//! Loader and integrity verifier for external `core-assets` declarative data.
//!
//! Prompts, recipes, and policy presets can be updated independently of
//! `evohime-core.exe` via the `core-assets` bundle. When the external bundle is
//! missing, malformed, or fails SHA-256 verification, Core safely falls back to
//! built-in defaults.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Expected schema identifier for `assets/core-assets/manifest.json`.
pub const CORE_ASSETS_SCHEMA: &str = "evohime.core-assets.v1";

/// Maximum allowed size (in bytes) of `manifest.json` inside `core-assets`.
pub const MAX_CORE_ASSETS_MANIFEST_BYTES: usize = 256 * 1024;

/// Directory layout declared inside `core-assets/manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreAssetsLayout {
    /// Relative directory containing prompt templates.
    pub prompts_dir: String,
    /// Relative directory containing capability/workflow recipes.
    pub recipes_dir: String,
    /// Relative directory containing declarative policy presets.
    pub policies_dir: String,
}

/// Parsed `core-assets` manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreAssetsManifest {
    /// Schema version string (`evohime.core-assets.v1`).
    pub schema: String,
    /// Semantic version of the asset bundle.
    pub bundle_version: String,
    /// Relative asset subdirectories.
    pub assets: CoreAssetsLayout,
}

/// Result of loading `core-assets` from disk with built-in fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadedCoreAssets {
    /// External `core-assets` bundle passed validation.
    External {
        /// Validated manifest.
        manifest: CoreAssetsManifest,
        /// Lowercase SHA-256 hex digest of `manifest.json`.
        manifest_sha256: String,
    },
    /// External bundle was absent or invalid; Core uses built-in defaults.
    BuiltinFallback {
        /// Reason why external assets were not used.
        reason: String,
    },
}

/// Loads and validates `core-assets/manifest.json` from `assets_root`.
///
/// If `expected_sha256` is provided, the manifest digest must match it
/// case-insensitively; otherwise falls back to [`LoadedCoreAssets::BuiltinFallback`].
pub fn load_core_assets_or_fallback(
    assets_root: &Path,
    expected_sha256: Option<&str>,
) -> LoadedCoreAssets {
    let manifest_path = assets_root.join("manifest.json");
    let bytes = match std::fs::read(&manifest_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            return LoadedCoreAssets::BuiltinFallback {
                reason: format!("cannot read {}: {err}", manifest_path.display()),
            };
        }
    };
    if bytes.len() > MAX_CORE_ASSETS_MANIFEST_BYTES {
        return LoadedCoreAssets::BuiltinFallback {
            reason: format!(
                "manifest size {} exceeds limit {MAX_CORE_ASSETS_MANIFEST_BYTES}",
                bytes.len()
            ),
        };
    }
    let digest = hex::encode(Sha256::digest(&bytes));
    if let Some(expected) = expected_sha256 {
        if !digest.eq_ignore_ascii_case(expected.trim()) {
            return LoadedCoreAssets::BuiltinFallback {
                reason: "manifest sha256 mismatch".to_owned(),
            };
        }
    }
    let manifest: CoreAssetsManifest = match serde_json::from_slice(&bytes) {
        Ok(manifest) => manifest,
        Err(err) => {
            return LoadedCoreAssets::BuiltinFallback {
                reason: format!("invalid manifest json: {err}"),
            };
        }
    };
    if manifest.schema != CORE_ASSETS_SCHEMA {
        return LoadedCoreAssets::BuiltinFallback {
            reason: format!("unsupported schema: {}", manifest.schema),
        };
    }
    if manifest.bundle_version.trim().is_empty() {
        return LoadedCoreAssets::BuiltinFallback {
            reason: "empty bundle_version".to_owned(),
        };
    }
    LoadedCoreAssets::External {
        manifest,
        manifest_sha256: digest,
    }
}

/// Loads a UTF-8 text asset (such as a prompt template) relative to
/// `assets_root`, rejecting path traversal or oversized files and falling back
/// to `builtin_default`.
pub fn load_text_asset_or_default(
    assets_root: &Path,
    relative_path: &str,
    builtin_default: &str,
) -> String {
    if relative_path.is_empty()
        || relative_path.contains("..")
        || relative_path.starts_with('/')
        || relative_path.starts_with('\\')
    {
        return builtin_default.to_owned();
    }
    let candidate = assets_root.join(relative_path);
    let Ok(bytes) = std::fs::read(candidate) else {
        return builtin_default.to_owned();
    };
    if bytes.len() > MAX_CORE_ASSETS_MANIFEST_BYTES {
        return builtin_default.to_owned();
    }
    String::from_utf8(bytes).unwrap_or_else(|_| builtin_default.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_valid_external_manifest_and_verifies_sha256() {
        let dir = tempfile::tempdir().expect("tempdir");
        let content = br#"{"schema":"evohime.core-assets.v1","bundle_version":"0.0.1","assets":{"prompts_dir":"prompts","recipes_dir":"recipes","policies_dir":"policies"}}"#;
        std::fs::write(dir.path().join("manifest.json"), content).expect("write manifest");

        let expected_hash = hex::encode(Sha256::digest(content));
        match load_core_assets_or_fallback(dir.path(), Some(&expected_hash)) {
            LoadedCoreAssets::External {
                manifest,
                manifest_sha256,
            } => {
                assert_eq!(manifest.bundle_version, "0.0.1");
                assert_eq!(manifest_sha256, expected_hash);
            }
            LoadedCoreAssets::BuiltinFallback { reason } => {
                panic!("unexpected fallback: {reason}");
            }
        }
    }

    #[test]
    fn falls_back_on_missing_or_mismatched_sha256() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(matches!(
            load_core_assets_or_fallback(dir.path(), None),
            LoadedCoreAssets::BuiltinFallback { .. }
        ));

        let content = br#"{"schema":"evohime.core-assets.v1","bundle_version":"0.0.1","assets":{"prompts_dir":"prompts","recipes_dir":"recipes","policies_dir":"policies"}}"#;
        std::fs::write(dir.path().join("manifest.json"), content).expect("write manifest");
        assert!(matches!(
            load_core_assets_or_fallback(dir.path(), Some("deadbeef")),
            LoadedCoreAssets::BuiltinFallback { .. }
        ));
    }
}
