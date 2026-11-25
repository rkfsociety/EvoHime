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
        let next_queen_working = !paused && queen_working;
        let changed = self.world.paused != paused
            || self.world.queen_working != next_queen_working
            || (paused && !self.world.active_dispatches.is_empty());

        self.world.paused = paused;
        self.world.queen_working = next_queen_working;
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
        }
        if changed {
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

        let unchanged = self.world.cities.len() == desired.len()
            && self
                .world
                .cities
                .iter()
                .zip(desired.iter())
                .enumerate()
                .all(|(idx, (city, (desired_id, desired_name)))| {
                    let is_primary = idx == 0;
                    city.id == *desired_id
                        && city.project_name == *desired_name
                        && city.is_primary == is_primary
                        && city.has_queen_citadel == is_primary
                });
        if unchanged && self.world.primary_city_id == desired[0].0 {
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
        let primary_city = self.world.cities.iter().find(|c| c.id == primary_id);
        let primary_max_ring = primary_city.map(|c| c.max_ring.max(1)).unwrap_or(1);
        let primary_outer_r = primary_city.map(city_outer_hex_radius).unwrap_or(0.0);

        let max_ext_outer_r = self
            .world
            .cities
            .iter()
            .filter(|c| c.id != primary_id)
            .map(city_outer_hex_radius)
            .fold(0.0_f64, f64::max);

        let ext_count = self.world.cities.len().saturating_sub(1);
        let min_dist_from_primary = primary_outer_r + max_ext_outer_r + INTERCITY_GAP_PX;
        let min_dist_between_exts = if ext_count >= 2 {
            (max_ext_outer_r * 2.0 + INTERCITY_GAP_PX)
                / (2.0 * (std::f64::consts::PI / (ext_count as f64)).sin())
        } else {
            0.0
        };
        let distance = min_dist_from_primary.max(min_dist_between_exts);
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

const CITADEL_HEX_RADIUS: f64 = 48.0;
const CELL_HEX_RADIUS: f64 = 30.0;
const RING_BASE_RADIUS: f64 = 84.0;
const RING_STEP: f64 = 112.0;
const INTERCITY_GAP_PX: f64 = 180.0;

fn select_citadel_exit_face_indices(exits_count: u8) -> &'static [usize] {
    match exits_count.clamp(1, 6) {
        1 => &[0],
        2 => &[0, 3],
        3 => &[0, 2, 4],
        4 => &[0, 1, 3, 4],
        5 => &[0, 1, 2, 3, 4],
        _ => &[0, 1, 2, 3, 4, 5],
    }
}

fn city_ring_expansion(city: &HoneycombCity) -> f64 {
    let sector_count = city.sectors.len();
    if sector_count == 0 {
        return 0.0;
    }
    let exits = if city.has_queen_citadel {
        city.citadel_exits_count.clamp(1, 6)
    } else {
        2
    };
    let face_angles = [
        std::f64::consts::FRAC_PI_6,
        std::f64::consts::FRAC_PI_2,
        5.0 * std::f64::consts::FRAC_PI_6,
        7.0 * std::f64::consts::FRAC_PI_6,
        3.0 * std::f64::consts::FRAC_PI_2,
        11.0 * std::f64::consts::FRAC_PI_6,
    ];
    let mut rays: Vec<f64> = select_citadel_exit_face_indices(exits)
        .iter()
        .map(|&idx| face_angles[idx].rem_euclid(std::f64::consts::TAU))
        .collect();
    rays.sort_by(|a, b| a.total_cmp(b));

    let mut spans = Vec::with_capacity(rays.len().max(1));
    if rays.is_empty() {
        spans.push(std::f64::consts::TAU);
    } else {
        for i in 0..rays.len() {
            let a = rays[i];
            let b = rays[(i + 1) % rays.len()];
            spans.push(if b > a {
                b - a
            } else {
                std::f64::consts::TAU - a + b
            });
        }
    }

    let mut counts = vec![0_usize; spans.len()];
    for _ in 0..sector_count {
        let mut best_idx = 0;
        let mut best_score = -1.0_f64;
        for (p, &span) in spans.iter().enumerate() {
            let score = span / ((counts[p] + 1) as f64);
            if score > best_score {
                best_score = score;
                best_idx = p;
            }
        }
        counts[best_idx] += 1;
    }

    let mut required_radius = 0.0_f64;
    for (p, &count) in counts.iter().enumerate() {
        if count == 0 {
            continue;
        }
        let min_arc_len = (count as f64) * 176.0 + 56.0;
        required_radius = required_radius.max(min_arc_len / spans[p].max(0.2));
    }
    (required_radius - (RING_BASE_RADIUS + RING_STEP)).max(0.0)
}

fn city_outer_hex_radius(city: &HoneycombCity) -> f64 {
    let module_max_ring = city.modules.iter().map(|m| m.ring).max().unwrap_or(0);
    let sector_min_ring = if city.sectors.is_empty() { 0 } else { 1 };
    let max_ring = city.max_ring.max(sector_min_ring).max(module_max_ring);
    let content_radius = if max_ring <= 0 {
        CITADEL_HEX_RADIUS + 28.0
    } else {
        let expansion = city_ring_expansion(city);
        RING_BASE_RADIUS + (max_ring as f64) * RING_STEP + expansion + CELL_HEX_RADIUS + 20.0
    };
    content_radius / (std::f64::consts::FRAC_PI_6).cos()
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

        // Pausing must clear active dispatches (vehicles parked in homes)
        // and force incremental delta readers to resync the paused state.
        let rev_before_pause = engine.world().revision;
        engine.set_active_mode(false, false, false).expect("pause");
        assert!(engine.world().paused);
        assert!(engine.world().active_dispatches.is_empty());
        let delta = engine
            .database()
            .read_delta_since(rev_before_pause, 256)
            .expect("delta");
        assert!(delta.requires_full_resync);
    }

    #[test]
    fn intercity_layout_keeps_gap_between_large_city_fences() {
        let db = HiveDatabase::open_in_memory().expect("in-memory db");
        let mut engine = HoneycombEngine::open(
            db,
            None,
            &[
                "C:/Projects/Primary".to_string(),
                "C:/Projects/Secondary".to_string(),
            ],
        )
        .expect("open engine");

        for city in &mut engine.world.cities {
            city.max_ring = 4;
        }
        engine.recompute_intercity_layout();

        let primary = &engine.world().cities[0];
        let secondary = &engine.world().cities[1];
        let dist = (secondary.world_center_x - primary.world_center_x)
            .hypot(secondary.world_center_y - primary.world_center_y);
        let min_non_overlapping = city_outer_hex_radius(primary) + city_outer_hex_radius(secondary);
        assert!(
            dist >= min_non_overlapping + INTERCITY_GAP_PX - 1e-6,
            "cities overlap: dist={dist}, sum_radii={min_non_overlapping}"
        );
    }
}
