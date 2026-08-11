use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

const TEXT_EXTENSIONS: &[&str] = &[
    "cs", "json", "md", "proto", "ps1", "rs", "toml", "txt", "xaml", "yaml", "yml",
];
const IGNORED_DIRECTORIES: &[&str] = &[".git", "target", "bin", "obj"];

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
    let references = input.references.join("\n");
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
    use super::{assemble_context, build_manifest, ContextInput};
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
            },
            80,
        );
        assert!(context.len() <= 80);
        assert!(context.contains("## Task"));
    }
}
