//! Bounded automation snapshots and side-effect-free simulation primitives.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Current serialized version for deterministic automation snapshots.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;
/// Maximum serialized size accepted for one snapshot.
pub const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;

/// Integrity-checked checkpoint of one automation run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationSnapshotV1 {
    /// Snapshot schema version.
    pub schema_version: u32,
    /// Identifier of the automation run.
    pub run_id: String,
    /// Identifier of the automation definition.
    pub definition_id: String,
    /// Definition revision used by this run.
    pub definition_revision: u64,
    /// Worker generation that owns this checkpoint.
    pub fencing_generation: u64,
    /// Last event sequence included in the checkpoint.
    pub last_event_sequence: u64,
    /// Serialized run state.
    pub state_json: String,
    /// Policy state bound to the checkpoint.
    pub policy_snapshot: String,
    /// Approval state bound to the checkpoint.
    pub approval_snapshot: String,
    /// Provenance reference for the checkpoint source.
    pub provenance: String,
    /// SHA-256 digest of all fields with this checksum cleared.
    pub checksum_sha256: String,
}

/// Snapshot size, integrity, compatibility, or freshness failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    /// Serialized snapshot exceeds the maximum size.
    Oversized,
    /// Checksum does not match the serialized snapshot contents.
    InvalidChecksum,
    /// Snapshot could not be serialized for validation or hashing.
    Serialization,
    /// Snapshot schema version is unsupported.
    IncompatibleSchema,
    /// Snapshot references a different automation definition revision.
    StaleDefinition,
    /// Fencing generation is invalid or event sequence moved backwards.
    InvalidGeneration,
    /// Snapshot lacks required provenance metadata.
    InvalidProvenance,
}
impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SnapshotError {}

impl AutomationSnapshotV1 {
    /// Creates a checkpoint and computes its integrity checksum.
    pub fn new(input: SnapshotInput<'_>) -> Result<Self, SnapshotError> {
        let mut snapshot = Self {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            run_id: input.run_id.into(),
            definition_id: input.definition_id.into(),
            definition_revision: input.definition_revision,
            fencing_generation: input.generation,
            last_event_sequence: input.sequence,
            state_json: input.state_json.into(),
            policy_snapshot: input.policy_snapshot.into(),
            approval_snapshot: input.approval_snapshot.into(),
            provenance: input.provenance.into(),
            checksum_sha256: String::new(),
        };
        snapshot.checksum_sha256 = snapshot.checksum()?;
        Ok(snapshot)
    }
    fn checksum(&self) -> Result<String, SnapshotError> {
        let mut unsigned = self.clone();
        unsigned.checksum_sha256.clear();
        let bytes = serde_json::to_vec(&unsigned).map_err(|_| SnapshotError::Serialization)?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }
    /// Checks size, checksum, definition revision, generation, sequence, and provenance.
    pub fn validate(
        &self,
        expected_definition_revision: u64,
        previous_sequence: Option<u64>,
    ) -> Result<(), SnapshotError> {
        if serde_json::to_vec(self)
            .map_err(|_| SnapshotError::Oversized)?
            .len()
            > MAX_SNAPSHOT_BYTES
        {
            return Err(SnapshotError::Oversized);
        }
        if self.schema_version != SNAPSHOT_SCHEMA_VERSION {
            return Err(SnapshotError::IncompatibleSchema);
        }
        if self.checksum_sha256 != self.checksum()? {
            return Err(SnapshotError::InvalidChecksum);
        }
        if self.definition_revision != expected_definition_revision {
            return Err(SnapshotError::StaleDefinition);
        }
        if self.fencing_generation == 0
            || previous_sequence.is_some_and(|previous| self.last_event_sequence < previous)
        {
            return Err(SnapshotError::InvalidGeneration);
        }
        if self.provenance.is_empty() {
            return Err(SnapshotError::InvalidProvenance);
        }
        Ok(())
    }
}

/// Borrowed fields used to create an owned automation snapshot.
pub struct SnapshotInput<'a> {
    /// Automation run identifier.
    pub run_id: &'a str,
    /// Automation definition identifier.
    pub definition_id: &'a str,
    /// Definition revision captured by the run.
    pub definition_revision: u64,
    /// Current fencing generation.
    pub generation: u64,
    /// Latest event sequence included in the state.
    pub sequence: u64,
    /// Serialized run state.
    pub state_json: &'a str,
    /// Policy snapshot bound to the run.
    pub policy_snapshot: &'a str,
    /// Approval snapshot bound to the run.
    pub approval_snapshot: &'a str,
    /// Provenance reference for the state.
    pub provenance: &'a str,
}

/// Inputs frozen for deterministic automation replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayInputV1 {
    /// Replay schema version.
    pub schema_version: u32,
    /// Definition revision being replayed.
    pub definition_revision: u64,
    /// Ordered event stream supplied to the replay.
    pub ordered_events: Vec<String>,
    /// Normalized request inputs.
    pub normalized_inputs: String,
    /// Frozen clock value in Unix milliseconds.
    pub frozen_clock_ms: i64,
    /// Seed for deterministic pseudo-random behavior.
    pub rng_seed: u64,
    /// Provider fixtures replacing live external providers.
    pub provider_fixture_ids: Vec<String>,
    /// Capability state frozen for replay.
    pub capability_snapshot: String,
    /// Policy state frozen for replay.
    pub policy_snapshot: String,
}
/// Hashes replay inputs to identify an equivalent deterministic simulation.
pub fn replay_hash(input: &ReplayInputV1) -> Result<String, SnapshotError> {
    if input.schema_version != SNAPSHOT_SCHEMA_VERSION {
        return Err(SnapshotError::IncompatibleSchema);
    }
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(input).map_err(|_| SnapshotError::InvalidProvenance)?,
    )))
}

/// Category of side effect a simulation might attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationEffect {
    /// Reads from or writes to the host filesystem.
    Filesystem,
    /// Connects to a live network resource.
    Network,
    /// Starts or signals a host process.
    Process,
    /// Executes a shell command.
    Shell,
    /// Reads or changes the host registry.
    Registry,
    /// Reads or changes the host clipboard.
    Clipboard,
    /// Sends a command to production IPC.
    ProductionIpc,
    /// Uses an isolated fake provider fixture.
    FakeProvider,
}
/// Returns whether an effect is permitted in side-effect-free simulation.
pub fn allow_simulation_effect(effect: SimulationEffect) -> bool {
    matches!(effect, SimulationEffect::FakeProvider)
}
/// Masks common bearer tokens and Windows absolute paths before export.
pub fn redact_export(value: &str) -> String {
    value
        .replace("Bearer ", "Bearer [REDACTED]")
        .replace("C:\\", "[ABSOLUTE_PATH]")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_checksum_and_revision_are_required() {
        let snapshot = AutomationSnapshotV1::new(SnapshotInput {
            run_id: "r",
            definition_id: "d",
            definition_revision: 1,
            generation: 1,
            sequence: 2,
            state_json: "{}",
            policy_snapshot: "p",
            approval_snapshot: "a",
            provenance: "prov",
        })
        .unwrap();
        assert!(snapshot.validate(1, Some(1)).is_ok());
        assert_eq!(
            snapshot.validate(2, None),
            Err(SnapshotError::StaleDefinition)
        );
    }
    #[test]
    fn replay_is_deterministic_and_simulation_denies_host_effects() {
        let input = ReplayInputV1 {
            schema_version: 1,
            definition_revision: 1,
            ordered_events: vec!["a".into()],
            normalized_inputs: "{}".into(),
            frozen_clock_ms: 10,
            rng_seed: 2,
            provider_fixture_ids: vec!["fixture".into()],
            capability_snapshot: "c".into(),
            policy_snapshot: "p".into(),
        };
        assert_eq!(replay_hash(&input).unwrap(), replay_hash(&input).unwrap());
        assert!(!allow_simulation_effect(SimulationEffect::Network));
        assert!(allow_simulation_effect(SimulationEffect::FakeProvider));
        assert!(redact_export("Bearer secret C:\\temp\\a").contains("REDACTED"));
    }
}
