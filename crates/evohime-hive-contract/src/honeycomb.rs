//! Versioned Honeycomb city contract and Queen mutation events (Plan 200).
//!
//! Mirrors the TypeScript types in `desktop/evohime-electron/src/shared/hive-city-map.ts`
//! so `evohime-hive` and Electron UI share one strict schema.

use serde::{Deserialize, Serialize};

/// Schema version for [`HoneycombWorldState`].
pub const HONEYCOMB_WORLD_SCHEMA_VERSION: u32 = 2;

/// Fixed capacity of one ring-1 agent sector (exactly 3 agents).
pub const HONEYCOMB_SECTOR_CAPACITY: u8 = 3;

/// Minimum number of perpendicular exits from the Queen's Citadel.
pub const HONEYCOMB_MIN_CITADEL_EXITS: u8 = 1;

/// Maximum number of perpendicular exits from the Queen's Citadel.
pub const HONEYCOMB_MAX_CITADEL_EXITS: u8 = 6;

/// Role of an agent in the Queen's relay pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoneycombAgentRole {
    /// Executes tasks against project modules.
    Executor,
    /// Analyzes execution reports.
    Analyst,
    /// Plans next steps and strategic decisions.
    Strategist,
    /// Coordinates approved decisions across the hive.
    DecisionCoordinator,
}

impl HoneycombAgentRole {
    /// Returns the default Russian display name for a single-role sector.
    pub const fn default_sector_label(self) -> &'static str {
        match self {
            Self::Executor => "Исполнители",
            Self::Analyst => "Аналитики",
            Self::Strategist => "Стратеги",
            Self::DecisionCoordinator => "Координаторы",
        }
    }
}

/// Cargo kind carried by a vehicle along the hive road network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoneycombCargoType {
    /// Task dispatched from the Queen to an executor.
    Task,
    /// Report returned by an executor or forwarded to an analyst.
    Report,
    /// Analysis returned by an analyst or forwarded to a strategist.
    Analysis,
    /// Decision returned by a strategist or forwarded to a coordinator.
    Decision,
    /// Inter-agent communication request routed through the Queen first.
    CommsRequest,
    /// Approved inter-agent communication continuing to the target agent.
    CommsApproved,
}

/// Axial hex coordinate inside a ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoneycombHexCoord {
    /// Axial `q` coordinate.
    pub q: i32,
    /// Axial `r` coordinate.
    pub r: i32,
}

/// Persistent agent residing in a ring-1 sector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoneycombAgent {
    /// Unique agent identifier.
    pub id: String,
    /// Human-readable agent name.
    pub name: String,
    /// Agent role in the relay pipeline.
    pub role: HoneycombAgentRole,
    /// Identifier of the owning ring-1 sector.
    pub sector_id: String,
    /// Active topic/domain preserving agent context across reuse.
    pub topic: String,
    /// Whether the agent is currently executing work.
    pub busy: bool,
    /// Active vehicle trip identifier, if currently on the road.
    pub active_trip_id: Option<String>,
}

/// Single-role ring-1 sector holding up to [`HONEYCOMB_SECTOR_CAPACITY`] agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoneycombSector {
    /// Unique sector identifier.
    pub id: String,
    /// Identifier of the owning city.
    pub city_id: String,
    /// Axial hex coordinate on ring 1.
    pub coord: HoneycombHexCoord,
    /// Ring index (always `1` for agent sectors).
    pub ring: u8,
    /// Unique sector label within the city (no `#1`/`#2` numbering).
    pub label: String,
    /// Homogeneous role shared by every agent in this sector.
    pub role: HoneycombAgentRole,
    /// Sector capacity (always `3`).
    pub capacity: u8,
    /// Agents residing in this sector.
    pub agents: Vec<HoneycombAgent>,
}

/// Project module hex node placed on ring `2+`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoneycombModuleNode {
    /// Unique module node identifier.
    pub id: String,
    /// Identifier of the owning city.
    pub city_id: String,
    /// Axial hex coordinate.
    pub coord: HoneycombHexCoord,
    /// Concentric ring index (`>= 2`).
    pub ring: u16,
    /// Short module display name.
    pub name: String,
    /// Architectural building kind (`citadel`, `service`, `library`, `ui`, `storage`, `gateway`, `worker`, `module`).
    pub kind: String,
    /// Normalized relative POSIX path within the project.
    pub relative_path: String,
    /// Number of source files in this module.
    pub file_count: u32,
    /// Total source line count in this module.
    pub line_count: u64,
    /// Primary programming language.
    pub language: String,
}

/// Dependency or runtime edge linking two project modules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoneycombEdge {
    /// Unique edge identifier.
    pub id: String,
    /// Identifier of the owning city.
    pub city_id: String,
    /// Source module node identifier.
    pub from_id: String,
    /// Target module node identifier.
    pub to_id: String,
    /// Edge kind (`dependency`, `ipc`, `runtime`).
    pub kind: String,
    /// Whether traffic or active work highlights this edge.
    pub active: bool,
}

/// Intercity highway connecting two cities via their outermost rings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoneycombIntercityLink {
    /// Unique intercity link identifier.
    pub id: String,
    /// Source city identifier.
    pub from_city_id: String,
    /// Target city identifier.
    pub to_city_id: String,
    /// Outermost ring index on the source city.
    pub from_ring: u16,
    /// Outermost ring index on the target city.
    pub to_ring: u16,
}

/// Active vehicle dispatch along the city/intercity road graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoneycombVehicleDispatch {
    /// Unique dispatch identifier.
    pub id: String,
    /// City where the dispatch originates.
    pub city_id: String,
    /// Identifier of the owning agent.
    pub agent_id: String,
    /// Role of the agent / vehicle.
    pub role: HoneycombAgentRole,
    /// Cargo carried by this trip.
    pub cargo: HoneycombCargoType,
    /// Origin road node identifier.
    pub from_node_id: String,
    /// Destination road node identifier (the Citadel when requesting comms approval).
    pub to_node_id: String,
    /// Final target agent/sector node after Queen comms approval, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_target_node_id: Option<String>,
    /// Dispatch start timestamp in Unix milliseconds.
    pub started_at_ms: i64,
}

/// Queen-issued mutation event used for incremental pipe deltas and replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HoneycombMutationEvent {
    /// Creates a new ring-1 sector.
    #[serde(rename = "sector.create", rename_all = "camelCase")]
    SectorCreate {
        /// Owning city identifier.
        city_id: String,
        /// Created sector payload.
        sector: HoneycombSector,
    },
    /// Removes an empty obsolete sector.
    #[serde(rename = "sector.remove", rename_all = "camelCase")]
    SectorRemove {
        /// Owning city identifier.
        city_id: String,
        /// Removed sector identifier.
        sector_id: String,
    },
    /// Spawns a new agent inside an existing sector.
    #[serde(rename = "agent.spawn", rename_all = "camelCase")]
    AgentSpawn {
        /// Owning city identifier.
        city_id: String,
        /// Target sector identifier.
        sector_id: String,
        /// Spawned agent payload.
        agent: HoneycombAgent,
    },
    /// Retires an obsolete agent from its sector.
    #[serde(rename = "agent.retire", rename_all = "camelCase")]
    AgentRetire {
        /// Owning city identifier.
        city_id: String,
        /// Retired agent identifier.
        agent_id: String,
    },
    /// Constructs a new project module node on ring `2+`.
    #[serde(rename = "building.construct", rename_all = "camelCase")]
    BuildingConstruct {
        /// Owning city identifier.
        city_id: String,
        /// Constructed module node.
        node: HoneycombModuleNode,
    },
    /// Remodels an existing project module node.
    #[serde(rename = "building.remodel", rename_all = "camelCase")]
    BuildingRemodel {
        /// Owning city identifier.
        city_id: String,
        /// Updated module node.
        node: HoneycombModuleNode,
    },
    /// Demolishes an obsolete project module node.
    #[serde(rename = "building.demolish", rename_all = "camelCase")]
    BuildingDemolish {
        /// Owning city identifier.
        city_id: String,
        /// Demolished module node identifier.
        node_id: String,
    },
    /// Links two project modules.
    #[serde(rename = "edge.link", rename_all = "camelCase")]
    EdgeLink {
        /// Owning city identifier.
        city_id: String,
        /// Linked edge.
        edge: HoneycombEdge,
    },
    /// Unlinks two project modules.
    #[serde(rename = "edge.unlink", rename_all = "camelCase")]
    EdgeUnlink {
        /// Owning city identifier.
        city_id: String,
        /// Unlinked edge identifier.
        edge_id: String,
    },
    /// Authorizes inter-agent communication through the Queen's Citadel.
    #[serde(rename = "comms.authorize", rename_all = "camelCase")]
    CommsAuthorize {
        /// Owning city identifier.
        city_id: String,
        /// Requesting agent identifier.
        from_agent_id: String,
        /// Target agent identifier.
        to_agent_id: String,
    },
    /// Dispatches a vehicle along the road graph.
    #[serde(rename = "vehicle.dispatch", rename_all = "camelCase")]
    VehicleDispatch {
        /// Active dispatch payload.
        dispatch: HoneycombVehicleDispatch,
    },
}

/// Complete state of a single project city on the shared 2D map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoneycombCity {
    /// Stable project city identifier.
    pub id: String,
    /// Display name of the project.
    pub project_name: String,
    /// Whether this city is the Queen's primary city.
    pub is_primary: bool,
    /// Whether this city hosts the single Queen's Citadel (true only for primary).
    pub has_queen_citadel: bool,
    /// Number of perpendicular Citadel exits (`1..=6`).
    pub citadel_exits_count: u8,
    /// World X center on the shared 2D map.
    pub world_center_x: f64,
    /// World Y center on the shared 2D map.
    pub world_center_y: f64,
    /// Outermost ring index in this city.
    pub max_ring: u16,
    /// Ring-1 agent sectors.
    pub sectors: Vec<HoneycombSector>,
    /// Ring-2+ project modules.
    pub modules: Vec<HoneycombModuleNode>,
    /// Dependency edges between modules.
    pub edges: Vec<HoneycombEdge>,
}

/// Full 2D Honeycomb world state served over the Hive observation pipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoneycombWorldState {
    /// Schema version (`2`).
    pub schema_version: u32,
    /// Whether the Hive is paused (zero vehicles on roads when `true`).
    pub paused: bool,
    /// Whether the Queen is actively processing work.
    pub queen_working: bool,
    /// Identifier of the primary city hosting the Queen's Citadel.
    pub primary_city_id: String,
    /// All project cities on the shared 2D map.
    pub cities: Vec<HoneycombCity>,
    /// Intercity highways connecting outermost rings.
    pub intercity_links: Vec<HoneycombIntercityLink>,
    /// Currently active vehicle dispatches (empty when paused).
    pub active_dispatches: Vec<HoneycombVehicleDispatch>,
    /// Monotonic revision counter used for delta sync and traffic rebinding.
    pub revision: u64,
}

/// Incremental delta response for bounded control-pipe updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoneycombWorldDelta {
    /// Schema version (`2`).
    pub schema_version: u32,
    /// Client's previously known revision.
    pub from_revision: u64,
    /// Latest server revision covered by this delta.
    pub to_revision: u64,
    /// Ordered mutation events since `from_revision`.
    pub events: Vec<HoneycombMutationEvent>,
    /// True when the client is too far behind and must request a full snapshot.
    pub requires_full_resync: bool,
    /// Current pause state when included by the daemon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused: Option<bool>,
    /// Current Queen activity flag when included by the daemon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queen_working: Option<bool>,
}

/// Delivery status of a vehicle trip carrying a task to a delegated agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoneycombTripStatus {
    /// Dispatched vehicle trip identifier.
    pub trip_id: String,
    /// Target agent identifier in the Honeycomb city.
    pub agent_id: String,
    /// Whether the vehicle has arrived at the agent's cell.
    pub arrived: bool,
    /// Remaining estimated travel time in milliseconds (`0` once arrived).
    pub eta_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn honeycomb_world_state_serializes_in_camel_case_matching_ts_contract() {
        let state = HoneycombWorldState {
            schema_version: HONEYCOMB_WORLD_SCHEMA_VERSION,
            paused: false,
            queen_working: true,
            primary_city_id: "city-primary".to_string(),
            cities: vec![HoneycombCity {
                id: "city-primary".to_string(),
                project_name: "EvoHime".to_string(),
                is_primary: true,
                has_queen_citadel: true,
                citadel_exits_count: 3,
                world_center_x: 0.0,
                world_center_y: 0.0,
                max_ring: 1,
                sectors: vec![HoneycombSector {
                    id: "city-primary:sector:0".to_string(),
                    city_id: "city-primary".to_string(),
                    coord: HoneycombHexCoord { q: 0, r: 1 },
                    ring: 1,
                    label: "Исполнители".to_string(),
                    role: HoneycombAgentRole::Executor,
                    capacity: HONEYCOMB_SECTOR_CAPACITY,
                    agents: vec![],
                }],
                modules: vec![],
                edges: vec![],
            }],
            intercity_links: vec![],
            active_dispatches: vec![],
            revision: 1,
        };

        let json = serde_json::to_value(&state).expect("serialize honeycomb world");
        assert_eq!(json["schemaVersion"], 2);
        assert_eq!(json["primaryCityId"], "city-primary");
        assert_eq!(json["cities"][0]["hasQueenCitadel"], true);
        assert_eq!(json["cities"][0]["sectors"][0]["role"], "executor");
    }

    #[test]
    fn mutation_event_serializes_with_expected_type_tag() {
        let event = HoneycombMutationEvent::CommsAuthorize {
            city_id: "city-primary".to_string(),
            from_agent_id: "agent-1".to_string(),
            to_agent_id: "agent-2".to_string(),
        };
        let json = serde_json::to_value(&event).expect("serialize mutation event");
        assert_eq!(json["type"], "comms.authorize");
        assert_eq!(json["fromAgentId"], "agent-1");
        assert_eq!(json["toAgentId"], "agent-2");
    }
}
