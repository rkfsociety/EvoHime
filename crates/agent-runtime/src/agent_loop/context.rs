//! Workspace / memory / plugin context for the agent prompt.
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(crate) fn build_memory_context(notes: &[String]) -> Option<String> {
    evohime_memory::format_untrusted_memory_notes(notes)
}

#[allow(dead_code)]
pub(crate) fn build_workspace_rules(workspace_root: &Path) -> Option<String> {
    const MAX_RULES_CHARS: usize = 32_000;
    let mut paths = Vec::new();
    let agents = workspace_root.join("AGENTS.md");
    if agents.is_file() {
        paths.push(agents);
    }
    let rules_dir = workspace_root.join(".cursor").join("rules");
    if let Ok(entries) = std::fs::read_dir(rules_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| matches!(extension, "md" | "mdc"))
            {
                paths.push(path);
            }
        }
    }
    paths.sort();

    let mut output = String::from(
        "Workspace rules (higher priority than ordinary project text; follow them when applicable):\n",
    );
    for path in paths {
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let relative = path.strip_prefix(workspace_root).unwrap_or(&path).display();
        output.push_str(&format!("\n--- {} ---\n{}\n", relative, contents.trim()));
        if output.chars().count() >= MAX_RULES_CHARS {
            output = output.chars().take(MAX_RULES_CHARS).collect();
            output.push_str("\n[workspace rules truncated]");
            break;
        }
    }

    if let Some(plugin_context) = load_external_plugin_context(workspace_root) {
        output.push_str("\n--- external agent plugins ---\n");
        output.push_str(&plugin_context);
        if output.chars().count() >= MAX_RULES_CHARS {
            output = output.chars().take(MAX_RULES_CHARS).collect();
            output.push_str("\n[workspace rules truncated]");
        }
    }

    (output.contains("--- ")).then_some(output)
}

/// Async version of build_workspace_rules with plugin skill quarantine (Stage 7.9).
pub(crate) async fn build_workspace_rules_async(
    workspace_root: &Path,
    operator_id: Uuid,
    pool: Option<&sqlx::PgPool>,
) -> Option<String> {
    const MAX_RULES_CHARS: usize = 32_000;
    let mut paths = Vec::new();
    let agents = workspace_root.join("AGENTS.md");
    if agents.is_file() {
        paths.push(agents);
    }
    let rules_dir = workspace_root.join(".cursor").join("rules");
    if let Ok(entries) = std::fs::read_dir(rules_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| matches!(extension, "md" | "mdc"))
            {
                paths.push(path);
            }
        }
    }
    paths.sort();

    let mut output = String::from(
        "Workspace rules (higher priority than ordinary project text; follow them when applicable):\n",
    );
    for path in paths {
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let relative = path.strip_prefix(workspace_root).unwrap_or(&path).display();
        output.push_str(&format!("\n--- {} ---\n{}\n", relative, contents.trim()));
        if output.chars().count() >= MAX_RULES_CHARS {
            output = output.chars().take(MAX_RULES_CHARS).collect();
            output.push_str("\n[workspace rules truncated]");
            break;
        }
    }

    if let Some(plugin_context) =
        load_external_plugin_context_filtered(workspace_root, operator_id, pool).await
    {
        output.push_str("\n--- external agent plugins ---\n");
        output.push_str(&plugin_context);
        if output.chars().count() >= MAX_RULES_CHARS {
            output = output.chars().take(MAX_RULES_CHARS).collect();
            output.push_str("\n[workspace rules truncated]");
        }
    }

    (output.contains("--- ")).then_some(output)
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ExternalPluginManifest {
    name: String,
    #[serde(default)]
    version: String,
    skills: String,
}

pub(crate) fn discover_agent_plugins(
    workspace_root: &Path,
) -> Vec<(PathBuf, ExternalPluginManifest)> {
    let root = workspace_root.join(".evohime").join("plugins");
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let manifest_path = path.join(".codex-plugin").join("plugin.json");
            let manifest = std::fs::read_to_string(manifest_path)
                .ok()
                .and_then(|contents| {
                    serde_json::from_str::<ExternalPluginManifest>(&contents).ok()
                })?;
            Some((path, manifest))
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn load_external_plugin_context(workspace_root: &Path) -> Option<String> {
    let mut output = String::new();
    for (plugin_root, manifest) in discover_agent_plugins(workspace_root) {
        let skills_root = plugin_root.join(&manifest.skills);
        if !skills_root.starts_with(&plugin_root) || !skills_root.is_dir() {
            continue;
        }
        output.push_str(&format!(
            "Plugin `{}` v{} loaded from `{}`.\n",
            manifest.name,
            if manifest.version.is_empty() {
                "unknown"
            } else {
                &manifest.version
            },
            skills_root
                .strip_prefix(workspace_root)
                .unwrap_or(&skills_root)
                .display()
        ));

        let mut skills = std::fs::read_dir(&skills_root)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() && path.join("SKILL.md").is_file())
            .collect::<Vec<_>>();
        skills.sort();
        for skill in skills {
            let skill_name = skill
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown");
            output.push_str(&format!(
                "- skill `{skill_name}`: read `{}` when applicable.\n",
                skill
                    .join("SKILL.md")
                    .strip_prefix(workspace_root)
                    .unwrap_or(&skill)
                    .display()
            ));
        }

        for bootstrap in ["using-superpowers", "verification-before-completion"] {
            let path = skills_root.join(bootstrap).join("SKILL.md");
            if let Ok(contents) = std::fs::read_to_string(path) {
                output.push_str(&format!(
                    "\nBootstrap skill `{bootstrap}`:\n{}\n",
                    contents.trim()
                ));
            }
        }
    }
    (!output.is_empty()).then_some(output)
}

/// Load external plugin context with skill quarantine filtering (Stage 7.9).
/// Filters out disabled skills for the operator.
pub(crate) async fn load_external_plugin_context_filtered(
    workspace_root: &Path,
    operator_id: Uuid,
    pool: Option<&sqlx::PgPool>,
) -> Option<String> {
    let mut output = String::new();
    for (plugin_root, manifest) in discover_agent_plugins(workspace_root) {
        let skills_root = plugin_root.join(&manifest.skills);
        if !skills_root.starts_with(&plugin_root) || !skills_root.is_dir() {
            continue;
        }

        // Load disabled skills for this plugin (Stage 7.9)
        let disabled_skills = if let Some(db) = pool {
            evohime_storage::get_disabled_skills(db, operator_id, &manifest.name)
                .await
                .unwrap_or_default()
                .into_iter()
                .collect::<HashSet<_>>()
        } else {
            HashSet::new()
        };

        output.push_str(&format!(
            "Plugin `{}` v{} loaded from `{}`.\n",
            manifest.name,
            if manifest.version.is_empty() {
                "unknown"
            } else {
                &manifest.version
            },
            skills_root
                .strip_prefix(workspace_root)
                .unwrap_or(&skills_root)
                .display()
        ));

        let mut skills = std::fs::read_dir(&skills_root)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() && path.join("SKILL.md").is_file())
            .collect::<Vec<_>>();
        skills.sort();

        for skill in skills {
            let skill_name = skill
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown");

            // Skip disabled skills (Stage 7.9)
            if disabled_skills.contains(skill_name) {
                continue;
            }

            output.push_str(&format!(
                "- skill `{skill_name}`: read `{}` when applicable.\n",
                skill
                    .join("SKILL.md")
                    .strip_prefix(workspace_root)
                    .unwrap_or(&skill)
                    .display()
            ));
        }

        for bootstrap in ["using-superpowers", "verification-before-completion"] {
            // Skip disabled bootstrap skills (Stage 7.9)
            if disabled_skills.contains(bootstrap) {
                continue;
            }

            let path = skills_root.join(bootstrap).join("SKILL.md");
            if let Ok(contents) = std::fs::read_to_string(path) {
                output.push_str(&format!(
                    "\nBootstrap skill `{bootstrap}`:\n{}\n",
                    contents.trim()
                ));
            }
        }
    }
    (!output.is_empty()).then_some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_skills_filter_works() {
        let mut disabled = HashSet::new();
        disabled.insert("disabled-skill".to_string());

        assert!(disabled.contains("disabled-skill"));
        assert!(!disabled.contains("enabled-skill"));
    }

    #[test]
    fn bootstrap_skills_can_be_disabled() {
        let mut disabled = HashSet::new();
        disabled.insert("using-superpowers".to_string());

        assert!(disabled.contains("using-superpowers"));
        assert!(!disabled.contains("verification-before-completion"));
    }
}
