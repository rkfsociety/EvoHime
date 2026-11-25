//! Queen's Honeycomb pipeline engine: role relay, sector lifecycle, comms authorization,
//! cooled-down city remodeling, and provider-limit parking (Plan 200).

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use evohime_hive_contract::honeycomb::{
    HoneycombAgent, HoneycombAgentRole, HoneycombCargoType, HoneycombCity, HoneycombEdge,
    HoneycombHexCoord, HoneycombIntercityLink, HoneycombModuleNode, HoneycombMutationEvent,
    HoneycombSector, HoneycombVehicleDispatch, HoneycombWorldState, HONEYCOMB_MAX_CITADEL_EXITS,
    HONEYCOMB_MIN_CITADEL_EXITS, HONEYCOMB_SECTOR_CAPACITY, HONEYCOMB_WORLD_SCHEMA_VERSION,
};

use crate::city_map::{scan_island, CityIslandRole, MAX_REPO_SCAN_DURATION};
use crate::honeycomb_db::{HiveDatabase, HiveDbError};

/// Minimum cooldown between full project city remodel passes (5 minutes).
pub const REMODEL_COOLDOWN_MS: i64 = 300_000;

/// Relay pipeline stage in the Queen's cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayStage {
    /// Executor carries out a task and produces a report.
    Execution,
    /// Analyst studies the report and produces an analysis.
    Analysis,
    /// Strategist evaluates the analysis and produces a decision.
    Strategy,
    /// Decision coordinator routes the decision to next tasks or city growth.
    DecisionCoordination,
}

impl RelayStage {
    /// Returns the agent role owning this stage.
    pub const fn role(self) -> HoneycombAgentRole {
        match self {
            Self::Execution => HoneycombAgentRole::Executor,
            Self::Analysis => HoneycombAgentRole::Analyst,
            Self::Strategy => HoneycombAgentRole::Strategist,
            Self::DecisionCoordination => HoneycombAgentRole::DecisionCoordinator,
        }
    }

    /// Returns the cargo delivered from the Queen to the agent at this stage.
    pub const fn inbound_cargo(self) -> HoneycombCargoType {
        match self {
            Self::Execution => HoneycombCargoType::Task,
            Self::Analysis => HoneycombCargoType::Report,
            Self::Strategy => HoneycombCargoType::Analysis,
            Self::DecisionCoordination => HoneycombCargoType::Decision,
        }
    }

    /// Advances to the next stage in the Queen's relay cycle.
    pub const fn next(self) -> Self {
        match self {
            Self::Execution => Self::Analysis,
            Self::Analysis => Self::Strategy,
            Self::Strategy => Self::DecisionCoordination,
            Self::DecisionCoordination => Self::Execution,
        }
    }
}

const EXTRA_SECTOR_LABELS: &[(HoneycombAgentRole, &str)] = &[
    (HoneycombAgentRole::Executor, "Разведчики"),
    (HoneycombAgentRole::Analyst, "Ревизоры"),
    (HoneycombAgentRole::Strategist, "Архитекторы"),
    (HoneycombAgentRole::DecisionCoordinator, "Интеграторы"),
];

/// Queen-owned Honeycomb state machine persisted in [`HiveDatabase`].
pub struct HoneycombEngine {
    db: HiveDatabase,
    world: HoneycombWorldState,
    last_remodel_at_ms: HashMap<String, i64>,
    last_structure_hashes: HashMap<String, String>,
    next_id_seq: u64,
}

impl HoneycombEngine {
    /// Loads existing world state from `db` or initializes a zero-start world
    /// (single Queen Citadel in the primary city).
    pub fn open(
        db: HiveDatabase,
        eva_workspace_path: Option<&str>,
        watched_repositories: &[String],
    ) -> Result<Self, HiveDbError> {
        let mut world = db.load_world_state(true, false, Vec::new())?;
        if world.cities.is_empty() {
            world = init_zero_world(eva_workspace_path, watched_repositories);
        }
        let mut engine = Self {
            db,
            world,
            last_remodel_at_ms: HashMap::new(),
            last_structure_hashes: HashMap::new(),
            next_id_seq: 1,
        };
        engine.recompute_intercity_layout();
        engine.db.save_world_state(&engine.world)?;
        Ok(engine)
    }

    /// Returns a read-only reference to the current world state.
    pub fn world(&self) -> &HoneycombWorldState {
        &self.world
    }

    /// Returns a read-only reference to the underlying database handle.
    pub fn database(&self) -> &HiveDatabase {
        &self.db
    }

    /// Updates pause / provider-limit state. When paused or limited, all vehicles
    /// are parked inside their sectors (`active_dispatches` becomes empty).
    pub fn set_active_mode(
        &mut self,
        autonomy_enabled: bool,
        waiting_for_limits: bool,
        queen_working: bool,
    ) -> Result<(), HiveDbError> {
        let paused = !autonomy_enabled || waiting_for_limits;
        self.world.paused = paused;
        self.world.queen_working = !paused && queen_working;
        if paused {
            self.world.active_dispatches.clear();
            for city in &mut self.world.cities {
                for sector in &mut city.sectors {
                    for agent in &mut sector.agents {
                        agent.busy = false;
                        agent.active_trip_id = None;
                    }
                }
            }
            self.world.revision = self.world.revision.saturating_add(1);
            self.db.save_world_state(&self.world)?;
        }
        Ok(())
    }

    /// Synchronizes configured project cities while keeping the single Queen Citadel
    /// in `eva_workspace_path` (priority) or the first watched repository.
    pub fn sync_projects(
        &mut self,
        eva_workspace_path: Option<&str>,
        watched_repositories: &[String],
    ) -> Result<(), HiveDbError> {
        let desired = collect_desired_projects(eva_workspace_path, watched_repositories);
        if desired.is_empty() {
            return Ok(());
        }

        let desired_ids: BTreeSet<String> = desired.iter().map(|(id, _)| id.clone()).collect();
        self.world.cities.retain(|c| desired_ids.contains(&c.id));

        for (idx, (city_id, project_name)) in desired.iter().enumerate() {
            let is_primary = idx == 0;
            if let Some(existing) = self.world.cities.iter_mut().find(|c| c.id == *city_id) {
                existing.project_name = project_name.clone();
                existing.is_primary = is_primary;
                existing.has_queen_citadel = is_primary;
            } else {
                self.world.cities.push(HoneycombCity {
                    id: city_id.clone(),
                    project_name: project_name.clone(),
                    is_primary,
                    has_queen_citadel: is_primary,
                    citadel_exits_count: if is_primary { 1 } else { 2 },
                    world_center_x: 0.0,
                    world_center_y: 0.0,
                    max_ring: 0,
                    sectors: Vec::new(),
                    modules: Vec::new(),
                    edges: Vec::new(),
                });
            }
            if is_primary {
                self.world.primary_city_id = city_id.clone();
            }
        }

        self.recompute_intercity_layout();
        self.world.revision = self.world.revision.saturating_add(1);
        self.db.save_world_state(&self.world)?;
        Ok(())
    }

    /// Runs one Queen relay step: reuses an agent with matching `(role, topic)` or
    /// spawns one into an existing/new homogeneous sector, then dispatches a vehicle.
    pub fn dispatch_relay_step(
        &mut self,
        stage: RelayStage,
        topic: &str,
        target_module_id: Option<&str>,
        now_ms: i64,
    ) -> Result<Vec<HoneycombMutationEvent>, HiveDbError> {
        if self.world.paused {
            return Ok(Vec::new());
        }

        let primary_id = self.world.primary_city_id.clone();
        let role = stage.role();
        let mut events = Vec::new();

        // 1. Try reusing an existing agent with matching `(role, topic)`.
        let mut chosen_agent_id: Option<String> = None;
        let mut chosen_sector_id: Option<String> = None;

        if let Some(city) = self.world.cities.iter().find(|c| c.id == primary_id) {
            for sector in &city.sectors {
                if sector.role != role {
                    continue;
                }
                if let Some(existing) = sector.agents.iter().find(|a| a.topic == topic) {
                    chosen_agent_id = Some(existing.id.clone());
                    chosen_sector_id = Some(sector.id.clone());
                    break;
                }
            }
        }

        // 2. Otherwise find a homogeneous sector with free capacity (< 3) or create one with a unique name.
        if chosen_agent_id.is_none() {
            let new_agent_id = self.alloc_id("agent");
            let sector_id = self.ensure_sector_for_role(&primary_id, role, &mut events)?;
            let agent = HoneycombAgent {
                id: new_agent_id.clone(),
                name: format!("Агент {}", new_agent_id),
                role,
                sector_id: sector_id.clone(),
                topic: topic.to_string(),
                busy: true,
                active_trip_id: None,
            };
            if let Some(city) = self.world.cities.iter_mut().find(|c| c.id == primary_id) {
                if let Some(sector) = city.sectors.iter_mut().find(|s| s.id == sector_id) {
                    sector.agents.push(agent.clone());
                }
            }
            events.push(HoneycombMutationEvent::AgentSpawn {
                city_id: primary_id.clone(),
                sector_id: sector_id.clone(),
                agent,
            });
            chosen_agent_id = Some(new_agent_id);
            chosen_sector_id = Some(sector_id);
        }

        let (agent_id, sector_id) = match (chosen_agent_id, chosen_sector_id) {
            (Some(a), Some(s)) => (a, s),
            _ => return Ok(events),
        };

        let trip_id = self.alloc_id("trip");
        let citadel_node = format!("{primary_id}:citadel");
        let destination = target_module_id
            .map(ToString::to_string)
            .unwrap_or_else(|| sector_id.clone());

        let dispatch = HoneycombVehicleDispatch {
            id: trip_id.clone(),
            city_id: primary_id.clone(),
            agent_id: agent_id.clone(),
            role,
            cargo: stage.inbound_cargo(),
            from_node_id: citadel_node,
            to_node_id: destination,
            final_target_node_id: None,
            started_at_ms: now_ms,
        };

        if let Some(city) = self.world.cities.iter_mut().find(|c| c.id == primary_id) {
            for sector in &mut city.sectors {
                if let Some(agent) = sector.agents.iter_mut().find(|a| a.id == agent_id) {
                    agent.busy = true;
                    agent.active_trip_id = Some(trip_id.clone());
                }
            }
            city.citadel_exits_count = compute_citadel_exits(city);
        }

        self.world.active_dispatches.push(dispatch.clone());
        events.push(HoneycombMutationEvent::VehicleDispatch { dispatch });
        self.commit_events(&events, now_ms)?;
        Ok(events)
    }

    /// Authorizes inter-agent communication: vehicle routes through the Queen's
    /// Citadel first (`comms_request`) before continuing to `to_sector_id`.
    pub fn authorize_inter_agent_comms(
        &mut self,
        from_agent_id: &str,
        to_agent_id: &str,
        now_ms: i64,
    ) -> Result<Vec<HoneycombMutationEvent>, HiveDbError> {
        if self.world.paused {
            return Ok(Vec::new());
        }

        let primary_id = self.world.primary_city_id.clone();
        let mut from_sector: Option<String> = None;
        let mut to_sector: Option<String> = None;
        let mut from_role = HoneycombAgentRole::Executor;

        if let Some(city) = self.world.cities.iter().find(|c| c.id == primary_id) {
            for sector in &city.sectors {
                for agent in &sector.agents {
                    if agent.id == from_agent_id {
                        from_sector = Some(sector.id.clone());
                        from_role = agent.role;
                    }
                    if agent.id == to_agent_id {
                        to_sector = Some(sector.id.clone());
                    }
                }
            }
        }

        let (origin_sector, target_sector) = match (from_sector, to_sector) {
            (Some(o), Some(t)) => (o, t),
            _ => return Ok(Vec::new()),
        };

        let trip_id = self.alloc_id("comms");
        let dispatch = HoneycombVehicleDispatch {
            id: trip_id,
            city_id: primary_id.clone(),
            agent_id: from_agent_id.to_string(),
            role: from_role,
            cargo: HoneycombCargoType::CommsRequest,
            from_node_id: origin_sector,
            to_node_id: format!("{primary_id}:citadel"),
            final_target_node_id: Some(target_sector),
            started_at_ms: now_ms,
        };

        let events = vec![
            HoneycombMutationEvent::CommsAuthorize {
                city_id: primary_id,
                from_agent_id: from_agent_id.to_string(),
                to_agent_id: to_agent_id.to_string(),
            },
            HoneycombMutationEvent::VehicleDispatch {
                dispatch: dispatch.clone(),
            },
        ];
        self.world.active_dispatches.push(dispatch);
        self.commit_events(&events, now_ms)?;
        Ok(events)
    }

    /// Retires an obsolete agent and deletes its sector if the sector becomes empty.
    pub fn retire_agent(
        &mut self,
        agent_id: &str,
        now_ms: i64,
    ) -> Result<Vec<HoneycombMutationEvent>, HiveDbError> {
        let mut events = Vec::new();
        for city in &mut self.world.cities {
            let mut empty_sector_ids = Vec::new();
            for sector in &mut city.sectors {
                let before = sector.agents.len();
                sector.agents.retain(|a| a.id != agent_id);
                if sector.agents.len() < before {
                    events.push(HoneycombMutationEvent::AgentRetire {
                        city_id: city.id.clone(),
                        agent_id: agent_id.to_string(),
                    });
                    if sector.agents.is_empty() {
                        empty_sector_ids.push(sector.id.clone());
                    }
                }
            }
            if !empty_sector_ids.is_empty() {
                city.sectors.retain(|s| !empty_sector_ids.contains(&s.id));
                for sector_id in empty_sector_ids {
                    events.push(HoneycombMutationEvent::SectorRemove {
                        city_id: city.id.clone(),
                        sector_id,
                    });
                }
            }
            if city.has_queen_citadel {
                city.citadel_exits_count = compute_citadel_exits(city);
            }
        }
        self.world
            .active_dispatches
            .retain(|d| d.agent_id != agent_id);
        if !events.is_empty() {
            self.commit_events(&events, now_ms)?;
        }
        Ok(events)
    }

    /// Scans a project checkout and remodels ring `2+` modules if the structure
    /// changed and the remodel cooldown has elapsed.
    pub fn remodel_city_from_checkout(
        &mut self,
        city_id: &str,
        checkout_path: &Path,
        now_ms: i64,
    ) -> Result<bool, HiveDbError> {
        if let Some(&last_ms) = self.last_remodel_at_ms.get(city_id) {
            if now_ms.saturating_sub(last_ms) < REMODEL_COOLDOWN_MS {
                return Ok(false);
            }
        }

        let Some(city) = self.world.cities.iter().find(|c| c.id == city_id) else {
            return Ok(false);
        };
        let role = if city.is_primary {
            CityIslandRole::Capital
        } else {
            CityIslandRole::Satellite
        };
        let island = match scan_island(
            checkout_path,
            city_id.to_string(),
            &city.project_name,
            role,
            Some(std::time::Instant::now() + MAX_REPO_SCAN_DURATION),
        ) {
            Ok(island) => island,
            Err(_) => return Ok(false),
        };

        if self.last_structure_hashes.get(city_id) == Some(&island.structure_hash) {
            return Ok(false);
        }

        let mut modules = Vec::new();
        for (idx, b) in island.buildings.iter().enumerate() {
            let ring: u16 = if idx < 6 {
                2
            } else {
                3 + ((idx - 6) / 10) as u16
            };
            modules.push(HoneycombModuleNode {
                id: format!("{city_id}:mod:{}", b.id),
                city_id: city_id.to_string(),
                coord: HoneycombHexCoord {
                    q: idx as i32,
                    r: i32::from(ring),
                },
                ring,
                name: b.name.clone(),
                kind: format!("{:?}", b.kind).to_lowercase(),
                relative_path: b.relative_path.clone(),
                file_count: b.file_count,
                line_count: b.line_count,
                language: b.language.clone(),
            });
        }

        let edges: Vec<HoneycombEdge> = island
            .edges
            .iter()
            .enumerate()
            .map(|(idx, e)| HoneycombEdge {
                id: format!("{city_id}:edge:{idx}"),
                city_id: city_id.to_string(),
                from_id: format!("{city_id}:mod:{}", e.from_id),
                to_id: format!("{city_id}:mod:{}", e.to_id),
                kind: format!("{:?}", e.kind).to_lowercase(),
                active: !self.world.paused,
            })
            .collect();

        if let Some(city_mut) = self.world.cities.iter_mut().find(|c| c.id == city_id) {
            let max_mod_ring = modules.iter().map(|m| m.ring).max().unwrap_or(1);
            city_mut.max_ring = max_mod_ring.max(if city_mut.sectors.is_empty() { 0 } else { 1 });
            city_mut.modules = modules;
            city_mut.edges = edges;
        }

        self.last_remodel_at_ms.insert(city_id.to_string(), now_ms);
        self.last_structure_hashes
            .insert(city_id.to_string(), island.structure_hash);
        self.recompute_intercity_layout();
        self.world.revision = self.world.revision.saturating_add(1);
        self.db.save_world_state(&self.world)?;
        Ok(true)
    }

    fn ensure_sector_for_role(
        &mut self,
        city_id: &str,
        role: HoneycombAgentRole,
        events: &mut Vec<HoneycombMutationEvent>,
    ) -> Result<String, HiveDbError> {
        let sector_id_existing =
            self.world
                .cities
                .iter()
                .find(|c| c.id == city_id)
                .and_then(|city| {
                    city.sectors
                        .iter()
                        .find(|s| {
                            s.role == role
                                && s.agents.len() < usize::from(HONEYCOMB_SECTOR_CAPACITY)
                        })
                        .map(|s| s.id.clone())
                });

        if let Some(id) = sector_id_existing {
            return Ok(id);
        }

        let used_labels: BTreeSet<String> = self
            .world
            .cities
            .iter()
            .find(|c| c.id == city_id)
            .map(|c| c.sectors.iter().map(|s| s.label.clone()).collect())
            .unwrap_or_default();

        let label = pick_unique_sector_label(role, &used_labels);
        let sector_id = self.alloc_id("sector");
        let coord_q = used_labels.len() as i32;
        let sector = HoneycombSector {
            id: sector_id.clone(),
            city_id: city_id.to_string(),
            coord: HoneycombHexCoord { q: coord_q, r: 1 },
            ring: 1,
            label,
            role,
            capacity: HONEYCOMB_SECTOR_CAPACITY,
            agents: Vec::new(),
        };

        if let Some(city) = self.world.cities.iter_mut().find(|c| c.id == city_id) {
            city.sectors.push(sector.clone());
            if city.max_ring < 1 {
                city.max_ring = 1;
            }
        }
        events.push(HoneycombMutationEvent::SectorCreate {
            city_id: city_id.to_string(),
            sector,
        });
        Ok(sector_id)
    }

    fn recompute_intercity_layout(&mut self) {
        self.world.intercity_links.clear();
        if self.world.cities.len() <= 1 {
            return;
        }
        let primary_id = self.world.primary_city_id.clone();
        let primary_max_ring = self
            .world
            .cities
            .iter()
            .find(|c| c.id == primary_id)
            .map(|c| c.max_ring.max(1))
            .unwrap_or(1);

        let ext_count = self.world.cities.len().saturating_sub(1);
        let distance = 1_280.0_f64;
        let mut ext_idx = 0_usize;

        for city in &mut self.world.cities {
            if city.id == primary_id {
                city.world_center_x = 0.0;
                city.world_center_y = 0.0;
                continue;
            }
            let angle = (ext_idx as f64) * std::f64::consts::TAU / (ext_count.max(1) as f64);
            city.world_center_x = angle.cos() * distance;
            city.world_center_y = angle.sin() * distance;
            ext_idx += 1;

            self.world.intercity_links.push(HoneycombIntercityLink {
                id: format!("link-{primary_id}-{}", city.id),
                from_city_id: primary_id.clone(),
                to_city_id: city.id.clone(),
                from_ring: primary_max_ring,
                to_ring: city.max_ring.max(1),
            });
        }
    }

    fn commit_events(
        &mut self,
        events: &[HoneycombMutationEvent],
        now_ms: i64,
    ) -> Result<(), HiveDbError> {
        self.world.revision = self.world.revision.saturating_add(1);
        self.db
            .append_mutation_events(self.world.revision, events, now_ms)?;
        self.db.save_world_state(&self.world)?;
        Ok(())
    }

    fn alloc_id(&mut self, prefix: &str) -> String {
        let seq = self.next_id_seq;
        self.next_id_seq = self.next_id_seq.saturating_add(1);
        format!("{prefix}-{seq}")
    }
}

fn pick_unique_sector_label(role: HoneycombAgentRole, used: &BTreeSet<String>) -> String {
    let primary = role.default_sector_label();
    if !used.contains(primary) {
        return primary.to_string();
    }
    for (candidate_role, label) in EXTRA_SECTOR_LABELS {
        if *candidate_role == role && !used.contains(*label) {
            return (*label).to_string();
        }
    }
    for (_, label) in EXTRA_SECTOR_LABELS {
        if !used.contains(*label) {
            return (*label).to_string();
        }
    }
    format!("{primary}-доп")
}

fn compute_citadel_exits(city: &HoneycombCity) -> u8 {
    let total_agents: usize = city.sectors.iter().map(|s| s.agents.len()).sum();
    let exits = total_agents.div_ceil(2);
    (exits as u8).clamp(HONEYCOMB_MIN_CITADEL_EXITS, HONEYCOMB_MAX_CITADEL_EXITS)
}

fn collect_desired_projects(
    eva_workspace_path: Option<&str>,
    watched_repositories: &[String],
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(eva_path) = eva_workspace_path.filter(|s| !s.trim().is_empty()) {
        out.push((
            "city-eva".to_string(),
            project_name_from_path(eva_path, "EvoHime"),
        ));
    }
    for (idx, repo) in watched_repositories.iter().enumerate() {
        if repo.trim().is_empty() {
            continue;
        }
        let id = format!("city-repo-{idx}");
        out.push((id, project_name_from_path(repo, "Project")));
    }
    if out.is_empty() {
        out.push(("city-eva".to_string(), "EvoHime".to_string()));
    }
    out
}

fn project_name_from_path(raw: &str, fallback: &str) -> String {
    Path::new(raw.trim())
        .file_name()
        .and_then(|s| s.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn init_zero_world(
    eva_workspace_path: Option<&str>,
    watched_repositories: &[String],
) -> HoneycombWorldState {
    let projects = collect_desired_projects(eva_workspace_path, watched_repositories);
    let mut cities = Vec::new();
    let mut primary_city_id = String::new();

    for (idx, (id, name)) in projects.into_iter().enumerate() {
        let is_primary = idx == 0;
        if is_primary {
            primary_city_id = id.clone();
        }
        cities.push(HoneycombCity {
            id,
            project_name: name,
            is_primary,
            has_queen_citadel: is_primary,
            citadel_exits_count: if is_primary { 1 } else { 2 },
            world_center_x: 0.0,
            world_center_y: 0.0,
            max_ring: 0,
            sectors: Vec::new(),
            modules: Vec::new(),
            edges: Vec::new(),
        });
    }

    HoneycombWorldState {
        schema_version: HONEYCOMB_WORLD_SCHEMA_VERSION,
        paused: true,
        queen_working: false,
        primary_city_id,
        cities,
        intercity_links: Vec::new(),
        active_dispatches: Vec::new(),
        revision: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_start_creates_only_single_queen_citadel_in_primary_city() {
        let db = HiveDatabase::open_in_memory().expect("in-memory db");
        let engine = HoneycombEngine::open(
            db,
            None,
            &[
                "C:/Projects/CodexGUI".to_string(),
                "C:/Projects/MemPulse".to_string(),
            ],
        )
        .expect("open engine");

        let world = engine.world();
        assert_eq!(world.cities.len(), 2);
        assert!(world.cities[0].has_queen_citadel);
        assert_eq!(world.cities[0].project_name, "CodexGUI");
        assert!(!world.cities[1].has_queen_citadel);
        assert!(world.cities[0].sectors.is_empty());
        assert!(world.active_dispatches.is_empty());
    }

    #[test]
    fn relay_pipeline_enforces_homogeneous_sectors_and_parks_on_pause() {
        let db = HiveDatabase::open_in_memory().expect("in-memory db");
        let mut engine = HoneycombEngine::open(db, Some("C:/github/EvoHime"), &[]).expect("open");
        engine.set_active_mode(true, false, true).expect("active");

        engine
            .dispatch_relay_step(RelayStage::Execution, "индексация", None, 1_000)
            .expect("exec");
        engine
            .dispatch_relay_step(RelayStage::Analysis, "отчёт-1", None, 2_000)
            .expect("analysis");
        assert_eq!(engine.world().cities[0].sectors.len(), 2);
        assert_eq!(engine.world().active_dispatches.len(), 2);

        // Pausing must clear active dispatches (vehicles parked in homes).
        engine.set_active_mode(false, false, false).expect("pause");
        assert!(engine.world().paused);
        assert!(engine.world().active_dispatches.is_empty());
    }
}
