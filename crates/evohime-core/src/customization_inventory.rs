//! Normalized metadata inventory over independently owned customization registries.
use serde::{Deserialize, Serialize};
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ITEMS: usize = 512;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomizationKind {
    Skill,
    Integration,
    Profile,
    Workflow,
    UiExtension,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomizationItem {
    pub schema_version: u32,
    pub id: String,
    pub kind: CustomizationKind,
    pub source: String,
    pub scope: String,
    pub version: u32,
    pub enabled: bool,
    pub compatibility: String,
    pub trust: String,
    pub health: String,
    pub content_hash: String,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InventoryError {
    #[error("unsupported inventory schema")]
    UnsupportedVersion,
    #[error("invalid inventory item")]
    InvalidItem,
    #[error("inventory bounds exceeded")]
    Bounds,
}
fn valid(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && !s.bytes().any(|b| b.is_ascii_control())
}
pub fn validate(item: &CustomizationItem) -> Result<(), InventoryError> {
    if item.schema_version != SCHEMA_VERSION {
        return Err(InventoryError::UnsupportedVersion);
    }
    if !valid(&item.id)
        || !valid(&item.source)
        || !valid(&item.scope)
        || !valid(&item.compatibility)
        || !valid(&item.trust)
        || !valid(&item.health)
        || item.version == 0
    {
        return Err(InventoryError::InvalidItem);
    }
    Ok(())
}
pub fn sort(items: &mut [CustomizationItem]) -> Result<(), InventoryError> {
    if items.len() > MAX_ITEMS {
        return Err(InventoryError::Bounds);
    }
    for i in items.iter() {
        validate(i)?
    }
    items.sort_by(|a, b| (a.kind as u8).cmp(&(b.kind as u8)).then(a.id.cmp(&b.id)));
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn i() -> CustomizationItem {
        CustomizationItem {
            schema_version: 1,
            id: "x".into(),
            kind: CustomizationKind::Skill,
            source: "builtin".into(),
            scope: "global".into(),
            version: 1,
            enabled: true,
            compatibility: "ok".into(),
            trust: "local".into(),
            health: "ready".into(),
            content_hash: "h".into(),
        }
    }
    #[test]
    fn normalized_sort_is_deterministic() {
        let mut x = vec![i()];
        sort(&mut x).unwrap();
        assert_eq!(x[0].id, "x")
    }
    #[test]
    fn invalid_version_rejected() {
        let mut x = i();
        x.version = 0;
        assert_eq!(validate(&x), Err(InventoryError::InvalidItem));
    }
}
