//! Content-hash lock file for installed plugins (Stage 7.102, wave 2).
//!
//! Subdir plugins lose `.git` during install, so a deterministic SHA-256
//! over the installed tree is the one integrity anchor that works for every
//! plugin the same way. The lock records what was installed and with which
//! trust level; the integrity check later tells whether it silently changed.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const LOCK_RELATIVE: &str = ".evohime/plugins.lock.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginLockEntry {
    pub name: String,
    pub version: String,
    pub source_url: Option<String>,
    pub git_ref: Option<String>,
    pub content_hash: String,
    pub trust_level: String,
    pub installed_at: DateTime<Utc>,
}

pub type PluginLock = BTreeMap<String, PluginLockEntry>;

pub fn lock_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(LOCK_RELATIVE)
}

/// Deterministic SHA-256 over the plugin tree: sorted relative paths, file
/// size and bytes. `.git` and symlinks are excluded; directory layout that
/// does not change file paths/contents does not change the hash.
pub fn content_hash(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();

    let mut hasher = Sha256::new();
    for relative in files {
        let full = root.join(&relative);
        let bytes = std::fs::read(&full)
            .map_err(|error| format!("hash read failed ({relative}): {error}"))?;
        hasher.update(relative.as_bytes());
        hasher.update([0u8]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    Ok(out)
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|error| format!("hash walk failed: {error}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            collect_files(root, &path, files)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            files.push(relative);
        }
    }
    Ok(())
}

/// Load the lock; a corrupted file yields an empty map with the flag set —
/// callers degrade to `unlocked`, never to a 500.
pub fn load_lock(workspace_root: &Path) -> (PluginLock, bool) {
    let path = lock_path(workspace_root);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return (PluginLock::new(), false);
    };
    match serde_json::from_str(&raw) {
        Ok(lock) => (lock, false),
        Err(_) => (PluginLock::new(), true),
    }
}

/// Atomic save: write a temp sibling, then rename over the target.
pub fn save_lock(workspace_root: &Path, lock: &PluginLock) -> Result<(), String> {
    let path = lock_path(workspace_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let serialized = serde_json::to_vec_pretty(lock).map_err(|error| error.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serialized).map_err(|error| error.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn record_install(workspace_root: &Path, entry: PluginLockEntry) -> Result<(), String> {
    let (mut lock, _) = load_lock(workspace_root);
    lock.insert(entry.name.clone(), entry);
    save_lock(workspace_root, &lock)
}

pub fn remove_entry(workspace_root: &Path, name: &str) -> Result<(), String> {
    let (mut lock, _) = load_lock(workspace_root);
    if lock.remove(name).is_some() {
        save_lock(workspace_root, &lock)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tree(dir: &Path) {
        std::fs::create_dir_all(dir.join("skills/deep")).expect("mkdir");
        std::fs::create_dir_all(dir.join(".git")).expect("mkdir git");
        std::fs::write(dir.join("skills/SKILL.md"), "content a").expect("write");
        std::fs::write(dir.join("skills/deep/notes.md"), "content b").expect("write");
        std::fs::write(dir.join(".git/config"), "ignored").expect("write");
    }

    #[test]
    fn hash_is_deterministic_and_ignores_git() {
        let first = tempfile::tempdir().expect("tempdir");
        let second = tempfile::tempdir().expect("tempdir");
        sample_tree(first.path());
        // Different creation order, extra .git content — same logical tree.
        std::fs::create_dir_all(second.path().join("skills/deep")).expect("mkdir");
        std::fs::write(second.path().join("skills/deep/notes.md"), "content b").expect("write");
        std::fs::write(second.path().join("skills/SKILL.md"), "content a").expect("write");
        std::fs::create_dir_all(second.path().join(".git/objects")).expect("mkdir git");
        std::fs::write(second.path().join(".git/HEAD"), "ref: other").expect("write");

        let hash_a = content_hash(first.path()).expect("hash a");
        let hash_b = content_hash(second.path()).expect("hash b");
        assert_eq!(hash_a, hash_b);
        assert_eq!(hash_a.len(), 64);
    }

    #[test]
    fn edits_and_renames_change_the_hash() {
        let dir = tempfile::tempdir().expect("tempdir");
        sample_tree(dir.path());
        let baseline = content_hash(dir.path()).expect("baseline");

        std::fs::write(dir.path().join("skills/SKILL.md"), "content A").expect("edit");
        let edited = content_hash(dir.path()).expect("edited");
        assert_ne!(baseline, edited);

        std::fs::write(dir.path().join("skills/SKILL.md"), "content a").expect("restore");
        assert_eq!(content_hash(dir.path()).expect("restored"), baseline);

        std::fs::rename(
            dir.path().join("skills/deep/notes.md"),
            dir.path().join("skills/deep/renamed.md"),
        )
        .expect("rename");
        assert_ne!(content_hash(dir.path()).expect("renamed"), baseline);
    }

    #[test]
    fn lock_round_trips_and_survives_corruption() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let entry = PluginLockEntry {
            name: "demo".into(),
            version: "1.0.0".into(),
            source_url: Some("https://github.com/a/b.git".into()),
            git_ref: None,
            content_hash: "abc".into(),
            trust_level: "curated".into(),
            installed_at: Utc::now(),
        };
        record_install(workspace.path(), entry.clone()).expect("record");
        let (lock, corrupted) = load_lock(workspace.path());
        assert!(!corrupted);
        assert_eq!(lock.get("demo"), Some(&entry));

        remove_entry(workspace.path(), "demo").expect("remove");
        let (lock, _) = load_lock(workspace.path());
        assert!(lock.is_empty());

        std::fs::write(lock_path(workspace.path()), "{ not json").expect("corrupt");
        let (lock, corrupted) = load_lock(workspace.path());
        assert!(lock.is_empty());
        assert!(corrupted);
    }
}
