//! Environment scrubbing for `shell.execute` (Stage 7.6).
//!
//! Default: allowlist of OS/tooling vars only (no API keys / DB URLs).
//! Always strip secret-looking names, even with inherit escape hatch.
//!
//! - `EVOHIME_SHELL_ENV_ALLOW` — comma-separated extra allowed names
//! - `EVOHIME_SHELL_INHERIT_ENV=1` — inherit parent env except secrets

use std::ffi::{OsStr, OsString};

const INHERIT_ENV: &str = "EVOHIME_SHELL_INHERIT_ENV";
const EXTRA_ALLOW_ENV: &str = "EVOHIME_SHELL_ENV_ALLOW";

/// Safe-by-default names (matched case-insensitively).
const DEFAULT_ALLOWLIST: &[&str] = &[
    "path",
    "pathext",
    "systemroot",
    "windir",
    "comspec",
    "temp",
    "tmp",
    "tmpdir",
    "userprofile",
    "homedrive",
    "homepath",
    "home",
    "username",
    "user",
    "logname",
    "appdata",
    "localappdata",
    "programdata",
    "programfiles",
    "programfiles(x86)",
    "number_of_processors",
    "processor_architecture",
    "processor_identifier",
    "os",
    "systemdrive",
    "lang",
    "language",
    "lc_all",
    "lc_ctype",
    "lc_messages",
    "term",
    "colorterm",
    "tz",
    "no_color",
    "force_color",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
    "cargo_home",
    "rustup_home",
    "cargo",
    "rustc",
];

const SECRET_EXACT: &[&str] = &[
    "database_url",
    "pgpassword",
    "github_token",
    "gh_token",
    "npm_token",
    "evohime_api_token",
];

const SECRET_PREFIXES: &[&str] = &[
    "literouter_",
    "openai_",
    "anthropic_",
    "aws_secret",
    "aws_access_key",
    "evohime_embedding_api",
];

const SECRET_SUBSTRINGS: &[&str] = &[
    "api_key",
    "apikey",
    "secret",
    "password",
    "passwd",
    "token",
    "credential",
    "private_key",
    "auth_key",
];

pub fn apply_scrubbed_env(command: &mut tokio::process::Command) {
    command.env_clear();
    for (key, value) in build_child_env(std::env::vars_os()) {
        command.env(key, value);
    }
}

pub fn build_child_env<I, K, V>(vars: I) -> Vec<(OsString, OsString)>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let inherit_all = env_flag_true(INHERIT_ENV);
    let extra = extra_allowlist();
    vars.into_iter()
        .filter(|(key, _)| {
            let name = key.as_ref().to_string_lossy();
            should_pass_env_key(&name, inherit_all, &extra)
        })
        .map(|(key, value)| (key.as_ref().to_os_string(), value.as_ref().to_os_string()))
        .collect()
}

pub fn should_pass_env_key(name: &str, inherit_all: bool, extra_allow: &[String]) -> bool {
    if is_secret_env_key(name) {
        return false;
    }
    if inherit_all {
        return true;
    }
    is_allowed_env_key(name, extra_allow)
}

pub fn is_secret_env_key(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }
    if SECRET_EXACT.iter().any(|exact| *exact == lower) {
        return true;
    }
    if SECRET_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return true;
    }
    SECRET_SUBSTRINGS
        .iter()
        .any(|needle| lower.contains(needle))
}

pub fn is_allowed_env_key(name: &str, extra_allow: &[String]) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    if DEFAULT_ALLOWLIST.iter().any(|allowed| *allowed == lower) {
        return true;
    }
    extra_allow.iter().any(|allowed| allowed == &lower)
}

fn extra_allowlist() -> Vec<String> {
    std::env::var(EXTRA_ALLOW_ENV)
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(|part| part.trim().to_ascii_lowercase())
                .filter(|part| !part.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn env_flag_true(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_common_secret_keys() {
        for key in [
            "LITEROUTER_API_KEY",
            "OPENAI_API_KEY",
            "DATABASE_URL",
            "EVOHIME_API_TOKEN",
            "GH_TOKEN",
            "my_password",
            "AWS_SECRET_ACCESS_KEY",
            "EVOHIME_EMBEDDING_API_KEY",
        ] {
            assert!(is_secret_env_key(key), "expected secret: {key}");
            assert!(!should_pass_env_key(key, false, &[]));
            assert!(
                !should_pass_env_key(key, true, &[]),
                "inherit must still scrub {key}"
            );
        }
    }

    #[test]
    fn allows_path_and_home() {
        assert!(should_pass_env_key("PATH", false, &[]));
        assert!(should_pass_env_key("Path", false, &[]));
        assert!(should_pass_env_key("HOME", false, &[]));
        assert!(should_pass_env_key("USERPROFILE", false, &[]));
        assert!(should_pass_env_key("TEMP", false, &[]));
        assert!(!should_pass_env_key("RANDOM_CUSTOM", false, &[]));
    }

    #[test]
    fn extra_allowlist_permits_named_vars() {
        let extra = vec!["my_project_flag".into()];
        assert!(should_pass_env_key("MY_PROJECT_FLAG", false, &extra));
        assert!(!should_pass_env_key("OTHER", false, &extra));
    }

    #[test]
    fn inherit_passes_non_secrets() {
        assert!(should_pass_env_key("CUSTOM_BUILD_FLAG", true, &[]));
        assert!(!should_pass_env_key("CUSTOM_API_KEY", true, &[]));
    }

    #[test]
    fn build_child_env_filters_map() {
        let vars = [
            ("PATH", "/bin"),
            ("LITEROUTER_API_KEY", "secret"),
            ("DATABASE_URL", "postgres://x"),
            ("HOME", "/home/dev"),
            ("CUSTOM", "nope"),
        ];
        let out = build_child_env(vars.iter().map(|(k, v)| (*k, *v)));
        let keys: Vec<String> = out
            .iter()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert!(keys.iter().any(|k| k.eq_ignore_ascii_case("PATH")));
        assert!(keys.iter().any(|k| k.eq_ignore_ascii_case("HOME")));
        assert!(!keys.iter().any(|k| k.contains("API_KEY")));
        assert!(!keys.iter().any(|k| k.eq_ignore_ascii_case("DATABASE_URL")));
        assert!(!keys.iter().any(|k| k == "CUSTOM"));
    }
}
