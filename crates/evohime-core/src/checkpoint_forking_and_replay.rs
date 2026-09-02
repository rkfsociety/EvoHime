use serde::{Deserialize, Serialize};
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_OVERRIDES: usize = 16;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForkRequest {
    pub schema_version: u32,
    pub source_checkpoint_id: String,
    pub parent_run_id: String,
    pub workspace_fingerprint: String,
    pub overrides: Vec<(String, String)>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForkLineage {
    pub schema_version: u32,
    pub fork_run_id: String,
    pub source_checkpoint_id: String,
    pub parent_run_id: String,
    pub workspace_fingerprint: String,
    pub replay_mode: String,
    pub effects_replayed: bool,
    pub overrides: Vec<(String, String)>,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ForkError {
    #[error("unsupported fork schema")]
    UnsupportedVersion,
    #[error("invalid fork request")]
    Invalid,
    #[error("fork bounds exceeded")]
    Bounds,
}
pub fn validate(r: &ForkRequest) -> Result<(), ForkError> {
    if r.schema_version != SCHEMA_VERSION {
        return Err(ForkError::UnsupportedVersion);
    }
    if r.source_checkpoint_id.is_empty()
        || r.parent_run_id.is_empty()
        || r.workspace_fingerprint.is_empty()
    {
        return Err(ForkError::Invalid);
    }
    if r.overrides.len() > MAX_OVERRIDES {
        return Err(ForkError::Bounds);
    }
    if r.overrides.iter().any(|(k, v)| {
        k.is_empty() || k.len() > 128 || v.len() > 512 || v.bytes().any(|b| b.is_ascii_control())
    }) {
        return Err(ForkError::Invalid);
    }
    Ok(())
}
pub fn create(r: ForkRequest, fork_run_id: String) -> Result<ForkLineage, ForkError> {
    validate(&r)?;
    Ok(ForkLineage {
        schema_version: 1,
        fork_run_id,
        source_checkpoint_id: r.source_checkpoint_id,
        parent_run_id: r.parent_run_id,
        workspace_fingerprint: r.workspace_fingerprint,
        replay_mode: "validated_boundary".into(),
        effects_replayed: false,
        overrides: r.overrides,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fork_never_replays_effects() {
        let x = create(
            ForkRequest {
                schema_version: 1,
                source_checkpoint_id: "c".into(),
                parent_run_id: "p".into(),
                workspace_fingerprint: "h".into(),
                overrides: vec![],
            },
            "f".into(),
        )
        .unwrap();
        assert!(!x.effects_replayed)
    }
    #[test]
    fn bounds_fail_closed() {
        let mut r = ForkRequest {
            schema_version: 1,
            source_checkpoint_id: "c".into(),
            parent_run_id: "p".into(),
            workspace_fingerprint: "h".into(),
            overrides: vec![],
        };
        r.overrides = vec![("x".into(), "y".into()); MAX_OVERRIDES + 1];
        assert_eq!(validate(&r), Err(ForkError::Bounds))
    }
}
