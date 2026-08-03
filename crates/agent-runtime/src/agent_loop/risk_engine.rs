use evohime_protocol::PlanStep;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    None = 0,
    Low = 1,
    Medium = 2,
    High = 3,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::None => "none",
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
        }
    }
}

/// Analyze planned steps and determine overall risk level
/// Risk levels:
/// - None: only read-only tools (filesystem.read, git.status, browser.open)
/// - Low: creating files in safe locations (temp, logs)
/// - Medium: modifying code (filesystem.patch, git.commit, filesystem.write to src)
/// - High: pushing to repos (git.push), shell with full commands, destructive operations
pub fn determine_risk_level(plan_steps: &[PlanStep]) -> RiskLevel {
    let mut max_risk = RiskLevel::None;

    for step in plan_steps {
        let tool_name = step.tool_name.as_str();

        let step_risk = match tool_name {
            // Read-only tools → None
            "filesystem.read" | "filesystem.search" | "filesystem.list" => RiskLevel::None,
            "git.status" | "git.diff" => RiskLevel::None,
            "browser.open" | "browser.extract" => RiskLevel::None,
            "browser.session.read" | "browser.session.screenshot" => RiskLevel::None,
            "memory.search" => RiskLevel::None,

            // Safe writes → Low (conservative: no params available, assume Medium)
            "filesystem.write" => RiskLevel::Medium,

            // Code modifications → Medium
            "filesystem.patch" => RiskLevel::Medium,
            "git.commit" => RiskLevel::Medium,

            // Network/repo operations → High
            "git.push" => RiskLevel::High,
            "git.pull" => RiskLevel::Low, // Pull is generally safe but can introduce changes

            // Shell is dangerous → High (conservative: no params to analyze)
            "shell.execute" => RiskLevel::High,

            // MCP calls and other tools
            "mcp.call" => RiskLevel::Medium,
            "http.fetch" => RiskLevel::Low,
            "browser.session.click" | "browser.session.type" => RiskLevel::Medium,
            "browser.session.navigate" => RiskLevel::Low,

            // Subagent execution → Medium (inherits child risks)
            "agent.run" => RiskLevel::Medium,
            "worker.run" => RiskLevel::Low,

            // Default: assume Medium if unknown
            _ => RiskLevel::Medium,
        };

        if step_risk > max_risk {
            max_risk = step_risk;
        }
    }

    max_risk
}

fn is_safe_write_path(path: &str) -> bool {
    let safe_patterns = [
        ".evohime/",      // Local cache/logs
        "target/",        // Build output
        ".git/",          // Git internals
        "node_modules/",  // Dependencies
        "__pycache__/",   // Python cache
        ".pytest_cache/", // Test cache
        "/tmp/",          // Temporary
        "\\tmp\\",        // Windows tmp
        "%TEMP%",         // Windows temp env
        "$TMPDIR",        // Unix temp env
    ];

    for pattern in &safe_patterns {
        if path.contains(pattern) {
            return true;
        }
    }

    false
}

fn is_dangerous_shell_command(cmd: &str) -> bool {
    let dangerous_patterns = [
        "rm -rf",
        "rm -r",
        "dd if=",
        "mkfs",
        "fdisk",
        "shutdown",
        "reboot",
        "kill -9",
        ":(){:|:&};:",  // fork bomb
        "sudo",
        "su ",
    ];

    for pattern in &dangerous_patterns {
        if cmd.contains(pattern) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::None < RiskLevel::Low);
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
    }

    #[test]
    fn test_is_safe_write_path() {
        assert!(is_safe_write_path(".evohime/cache.json"));
        assert!(is_safe_write_path("target/debug/app"));
        assert!(is_safe_write_path("/tmp/tmpfile"));
        assert!(!is_safe_write_path("src/main.rs"));
        assert!(!is_safe_write_path("package.json"));
    }

    #[test]
    fn test_is_dangerous_shell_command() {
        assert!(is_dangerous_shell_command("rm -rf /"));
        assert!(is_dangerous_shell_command("sudo apt-get install"));
        assert!(is_dangerous_shell_command("dd if=/dev/zero of=/dev/sda"));
        assert!(!is_dangerous_shell_command("grep 'pattern' file.txt"));
        assert!(!is_dangerous_shell_command("ls -la"));
    }
}
