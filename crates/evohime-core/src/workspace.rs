use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

const TEXT_EXTENSIONS: &[&str] = &[
    "cs", "json", "md", "proto", "ps1", "rs", "toml", "txt", "xaml", "yaml", "yml",
];
const IGNORED_DIRECTORIES: &[&str] = &[
    ".git",
    ".evohime",
    ".evohime-native",
    ".launcher-logs",
    "artifacts",
    "bin",
    "build",
    "dist",
    "node_modules",
    "obj",
    "target",
];
pub const MAX_LIST_ENTRIES: usize = 200;
pub const MAX_READ_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManifestEntry {
    pub relative_path: String,
    pub bytes: usize,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceManifest {
    pub entries: Vec<ManifestEntry>,
    pub workspace_hash: String,
    pub total_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextInput<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub acceptance_criteria: &'a str,
    pub non_goals: &'a str,
    pub references: &'a [String],
    /// Context contributed by installed skills, as `(skill_name, content)`
    /// pairs. Assembly never trusts caller-supplied order: entries are
    /// always sorted by `skill_name` before being rendered into a single
    /// "Skill context" section at a fixed position in the output (after
    /// non-goals, before workspace references). This guarantees two runs
    /// with the same skill set produce byte-identical context regardless
    /// of registry iteration order or which skill matched first, and that
    /// no skill can push its content earlier/later relative to task fields
    /// by controlling insertion order.
    pub skill_context: &'a [(String, String)],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceEntry {
    pub name: String,
    pub relative_path: String,
    pub directory: bool,
    pub bytes: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceListing {
    pub path: String,
    pub entries: Vec<WorkspaceEntry>,
    pub truncated: bool,
}

pub fn list_directory(
    root: impl AsRef<Path>,
    relative_path: &str,
    max_entries: usize,
) -> std::io::Result<WorkspaceListing> {
    if max_entries == 0 || max_entries > MAX_LIST_ENTRIES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("workspace listing limit must be between 1 and {MAX_LIST_ENTRIES}"),
        ));
    }
    let root = root.as_ref().canonicalize()?;
    let directory = resolve_inside(&root, relative_path)?;
    if !directory.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "workspace path is not a directory",
        ));
    }

    let mut entries: Vec<WorkspaceEntry> = fs::read_dir(&directory)?
        .map(|entry| {
            let entry = entry?;
            if is_ignored_directory(&entry) {
                return Ok(None);
            }
            let path = entry.path();
            let metadata = entry.metadata()?;
            let relative_path = path
                .strip_prefix(&root)
                .expect("workspace entry is inside root")
                .to_string_lossy()
                .replace('\\', "/");
            Ok(Some(WorkspaceEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                relative_path,
                directory: metadata.is_dir(),
                bytes: metadata.is_file().then_some(metadata.len() as usize),
            }))
        })
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    entries.sort_by(|left, right| {
        right.directory.cmp(&left.directory).then_with(|| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
        })
    });
    let truncated = entries.len() > max_entries;
    entries.truncate(max_entries);

    Ok(WorkspaceListing {
        path: normalize_relative_path(relative_path),
        entries,
        truncated,
    })
}

pub fn read_text_file(
    root: impl AsRef<Path>,
    relative_path: &str,
    max_bytes: usize,
) -> std::io::Result<String> {
    if max_bytes == 0 || max_bytes > MAX_READ_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("workspace read limit must be between 1 and {MAX_READ_BYTES}"),
        ));
    }
    let root = root.as_ref().canonicalize()?;
    let path = resolve_inside(&root, relative_path)?;
    if !path.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "workspace path is not a file",
        ));
    }
    let content = fs::read(&path)?;
    if content.len() > max_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "workspace file exceeds the read limit",
        ));
    }
    String::from_utf8(content)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error.utf8_error()))
}

fn resolve_inside(root: &Path, relative_path: &str) -> std::io::Result<PathBuf> {
    let candidate = root.join(relative_path);
    let resolved = candidate.canonicalize()?;
    if resolved.starts_with(root) {
        Ok(resolved)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "workspace path escapes the selected workspace",
        ))
    }
}

fn normalize_relative_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if normalized.is_empty() {
        ".".to_string()
    } else {
        normalized
    }
}

fn is_ignored_directory(entry: &fs::DirEntry) -> bool {
    entry
        .file_type()
        .map(|kind| {
            kind.is_dir()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| IGNORED_DIRECTORIES.contains(&name))
        })
        .unwrap_or(false)
}

pub fn build_manifest(
    root: impl AsRef<Path>,
    max_files: usize,
    max_bytes: usize,
) -> std::io::Result<WorkspaceManifest> {
    let root = root.as_ref().canonicalize()?;
    let mut paths = Vec::new();
    collect_paths(&root, &mut paths)?;
    paths.sort();
    let mut entries = Vec::new();
    let mut total_bytes: usize = 0;
    for path in paths.into_iter().take(max_files) {
        let content = fs::read(&path)?;
        if total_bytes.saturating_add(content.len()) > max_bytes {
            break;
        }
        let relative_path = path
            .strip_prefix(&root)
            .expect("manifest path is inside root")
            .to_string_lossy()
            .replace('\\', "/");
        total_bytes += content.len();
        entries.push(ManifestEntry {
            relative_path,
            bytes: content.len(),
            content_hash: content_hash(&content),
        });
    }
    let canonical = entries
        .iter()
        .map(|entry| {
            format!(
                "{}:{}:{}\n",
                entry.relative_path, entry.bytes, entry.content_hash
            )
        })
        .collect::<String>();
    Ok(WorkspaceManifest {
        entries,
        workspace_hash: content_hash(canonical.as_bytes()),
        total_bytes,
    })
}

pub fn assemble_context(input: ContextInput<'_>, max_chars: usize) -> String {
    let mut sections = vec![
        ("Task", input.title),
        ("Description", input.description),
        ("Acceptance criteria", input.acceptance_criteria),
        ("Non-goals", input.non_goals),
    ];
    // Deterministic, caller-order-independent rendering: sort by skill
    // name so registry/matcher iteration order can never change output.
    let mut skill_entries = input.skill_context.to_vec();
    skill_entries.sort_by(|a, b| a.0.cmp(&b.0));
    let skill_context = skill_entries
        .iter()
        .map(|(name, content)| format!("### {name}\n{}", content.trim()))
        .collect::<Vec<_>>()
        .join("\n\n");
    let references = input.references.join("\n");
    sections.push(("Skill context", &skill_context));
    sections.push(("Workspace references", &references));
    let mut output = String::new();
    for (label, value) in sections {
        if value.trim().is_empty() {
            continue;
        }
        let section = format!("## {label}\n{}\n\n", value.trim());
        if output.len() + section.len() > max_chars {
            let remaining = max_chars.saturating_sub(output.len());
            output.push_str(&section.chars().take(remaining).collect::<String>());
            break;
        }
        output.push_str(&section);
    }
    output
}

fn collect_paths(root: &Path, paths: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| IGNORED_DIRECTORIES.contains(&name))
            {
                continue;
            }
            collect_paths(&path, paths)?;
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| TEXT_EXTENSIONS.contains(&ext))
        {
            paths.push(path);
        }
    }
    Ok(())
}

pub fn content_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::{
        assemble_context, build_manifest, list_directory, read_text_file, ContextInput,
        MAX_LIST_ENTRIES, MAX_READ_BYTES,
    };
    use std::{fs, path::PathBuf};

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("evohime-workspace-{name}-{}", std::process::id()))
    }

    #[test]
    fn manifest_is_deterministic_and_ignores_build_directories() {
        let root = temp_root("manifest");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("ignored.bin"), [1, 2, 3]).unwrap();
        fs::write(root.join("target/generated.rs"), "generated").unwrap();
        let first = build_manifest(&root, 10, 1000).unwrap();
        let second = build_manifest(&root, 10, 1000).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.entries.len(), 1);
        assert_eq!(first.entries[0].relative_path, "main.rs");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_respects_file_and_byte_bounds() {
        let root = temp_root("bounds");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), "12345").unwrap();
        fs::write(root.join("b.txt"), "67890").unwrap();
        let manifest = build_manifest(&root, 1, 6).unwrap();
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(manifest.total_bytes, 5);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn context_assembler_is_bounded_and_contains_task_sections() {
        let references = vec!["src/main.rs".into()];
        let context = assemble_context(
            ContextInput {
                title: "Implement feature",
                description: "A bounded context",
                acceptance_criteria: "Tests pass",
                non_goals: "No network",
                references: &references,
                skill_context: &[],
            },
            80,
        );
        assert!(context.len() <= 80);
        assert!(context.contains("## Task"));
    }

    #[test]
    fn skill_context_is_ordered_deterministically_regardless_of_input_order() {
        let references: Vec<String> = Vec::new();
        let base = |skill_context: &[(String, String)]| {
            assemble_context(
                ContextInput {
                    title: "Implement feature",
                    description: "A bounded context",
                    acceptance_criteria: "Tests pass",
                    non_goals: "No network",
                    references: &references,
                    skill_context,
                },
                4_096,
            )
        };
        let forward = vec![
            ("zeta".to_string(), "zeta content".to_string()),
            ("alpha".to_string(), "alpha content".to_string()),
        ];
        let reversed = vec![
            ("alpha".to_string(), "alpha content".to_string()),
            ("zeta".to_string(), "zeta content".to_string()),
        ];
        let first = base(&forward);
        let second = base(&reversed);
        assert_eq!(first, second, "skill context order must not depend on caller-supplied insertion order");
        assert!(first.contains("## Skill context"));
        let alpha_pos = first.find("### alpha").unwrap();
        let zeta_pos = first.find("### zeta").unwrap();
        let skill_pos = first.find("## Skill context").unwrap();
        let refs_pos = first.find("## Workspace references");
        assert!(alpha_pos < zeta_pos, "entries must be sorted by skill name");
        assert!(skill_pos > first.find("## Non-goals").unwrap(), "skill context is fixed after non-goals");
        if let Some(refs_pos) = refs_pos {
            assert!(skill_pos < refs_pos, "skill context is fixed before workspace references");
        }
    }

    #[test]
    fn context_assembly_is_deterministic_across_repeated_runs_with_same_inputs() {
        let references = vec!["src/main.rs".into(), "src/lib.rs".into()];
        let skills = vec![
            ("reviewer".to_string(), "Review the diff".to_string()),
            ("planner".to_string(), "Plan the next step".to_string()),
        ];
        let make = || {
            assemble_context(
                ContextInput {
                    title: "Implement feature",
                    description: "A bounded context",
                    acceptance_criteria: "Tests pass",
                    non_goals: "No network",
                    references: &references,
                    skill_context: &skills,
                },
                4_096,
            )
        };
        assert_eq!(make(), make());
    }

    #[test]
    fn lists_directories_before_files_and_ignores_build_output() {
        let root = temp_root("listing");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::create_dir_all(root.join(".evohime-native")).unwrap();
        fs::write(root.join("README.md"), "readme").unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

        let listing = list_directory(&root, ".", 20).unwrap();

        assert_eq!(listing.path, ".");
        assert_eq!(
            listing
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["src", "README.md"]
        );
        assert!(!listing.truncated);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn listing_reports_truncation_and_rejects_escape() {
        let root = temp_root("listing-bounds");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), "a").unwrap();
        fs::write(root.join("b.txt"), "b").unwrap();

        let listing = list_directory(&root, ".", 1).unwrap();
        assert_eq!(listing.entries.len(), 1);
        assert!(listing.truncated);
        assert!(list_directory(&root, ".", MAX_LIST_ENTRIES + 1).is_err());
        assert!(list_directory(&root, "..", 10).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_bounded_utf8_file_and_rejects_binary_or_oversized_input() {
        let root = temp_root("read");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("note.txt"), "hello\nworld").unwrap();
        fs::write(root.join("binary.dat"), [0, 159, 146, 150]).unwrap();

        assert_eq!(
            read_text_file(&root, "note.txt", 100).unwrap(),
            "hello\nworld"
        );
        assert!(read_text_file(&root, "note.txt", 5).is_err());
        assert!(read_text_file(&root, "note.txt", MAX_READ_BYTES + 1).is_err());
        assert!(read_text_file(&root, "binary.dat", 100).is_err());
        assert!(read_text_file(&root, "../note.txt", 100).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
