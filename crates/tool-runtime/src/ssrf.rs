//! SSRF guards for outbound HTTP tools (Stage 7.4+).
//!
//! Blocks loopback, private, link-local, metadata, and non-http(s) schemes.
//! Escape hatch: `EVOHIME_SSRF_ALLOW_PRIVATE=1` (local power users / tests).
//! Optional host allowlist via env (e.g. `EVOHIME_MCP_ALLOWED_HOSTS`).

use reqwest::Url;
use std::cell::{Cell, RefCell};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};

const BLOCKED_HOSTNAMES: &[&str] = &[
    "localhost",
    "localhost.localdomain",
    "metadata",
    "metadata.google.internal",
    "metadata.goog",
];

thread_local! {
    /// Per-thread override for tests (avoids global mutex reentrancy / races).
    static PRIVATE_OVERRIDE: Cell<Option<bool>> = const { Cell::new(None) };
    /// `None` = read env; `Some(None)` = no allowlist; `Some(Some(hosts))` = forced list.
    static HOST_ALLOWLIST_OVERRIDE: RefCell<Option<Option<Vec<String>>>> =
        const { RefCell::new(None) };
}

/// Restores the previous per-thread private-target override on drop.
pub struct PrivateOverrideGuard {
    previous: Option<bool>,
}

impl Drop for PrivateOverrideGuard {
    fn drop(&mut self) {
        PRIVATE_OVERRIDE.with(|cell| cell.set(self.previous));
    }
}

/// Set private-target override for the current thread until the guard is dropped.
pub fn lock_private_override(value: Option<bool>) -> PrivateOverrideGuard {
    PRIVATE_OVERRIDE.with(|cell| {
        let previous = cell.get();
        cell.set(value);
        PrivateOverrideGuard { previous }
    })
}

pub fn allow_private_targets() -> bool {
    if let Some(value) = PRIVATE_OVERRIDE.with(|cell| cell.get()) {
        return value;
    }
    std::env::var("EVOHIME_SSRF_ALLOW_PRIVATE")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// Restores the previous per-thread host allowlist override on drop.
pub struct HostAllowlistGuard {
    previous: Option<Option<Vec<String>>>,
}

impl Drop for HostAllowlistGuard {
    fn drop(&mut self) {
        HOST_ALLOWLIST_OVERRIDE.with(|cell| {
            *cell.borrow_mut() = self.previous.take();
        });
    }
}

/// Set host allowlist override for the current thread until the guard is dropped.
///
/// Pass `None` to force “no allowlist”; `Some(hosts)` to require those hosts.
pub fn lock_host_allowlist(hosts: Option<Vec<String>>) -> HostAllowlistGuard {
    HOST_ALLOWLIST_OVERRIDE.with(|cell| {
        let previous = cell.borrow().clone();
        *cell.borrow_mut() = Some(hosts);
        HostAllowlistGuard { previous }
    })
}

/// Parse comma-separated host allowlist from an env var. Empty / unset → `None`.
pub fn host_allowlist_from_env(var: &str) -> Option<Vec<String>> {
    let raw = std::env::var(var).ok()?;
    let hosts = parse_host_allowlist(&raw);
    if hosts.is_empty() {
        None
    } else {
        Some(hosts)
    }
}

pub fn parse_host_allowlist(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|part| part.trim().trim_end_matches('.').to_ascii_lowercase())
        .filter(|part| !part.is_empty())
        .collect()
}

/// Effective allowlist: thread override, else env var.
pub fn effective_host_allowlist(env_var: &str) -> Option<Vec<String>> {
    if let Some(override_value) = HOST_ALLOWLIST_OVERRIDE.with(|cell| cell.borrow().clone()) {
        return override_value;
    }
    host_allowlist_from_env(env_var)
}

pub fn assert_host_in_allowlist(url: &Url, hosts: &[String]) -> Result<(), String> {
    let Some(host) = url.host_str() else {
        return Err("url host is required".into());
    };
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    if hosts.iter().any(|allowed| allowed == &normalized) {
        Ok(())
    } else {
        Err(format!("host not in allowlist: {host}"))
    }
}

/// Validate scheme/host/IP (and DNS resolution for domain names).
pub fn assert_safe_http_url(url: &Url) -> Result<(), String> {
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err("url must use http or https".into()),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("url must not include credentials".into());
    }
    let Some(host) = url.host_str() else {
        return Err("url host is required".into());
    };
    if allow_private_targets() {
        return Ok(());
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return assert_safe_ip(ip);
    }
    assert_safe_hostname(host)?;
    let port = url.port_or_known_default().unwrap_or(80);
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|error| format!("dns lookup failed for {host}: {error}"))?;
    let mut saw_any = false;
    for addr in addrs {
        saw_any = true;
        assert_safe_ip(addr.ip())?;
    }
    if !saw_any {
        return Err(format!("dns lookup returned no addresses for {host}"));
    }
    Ok(())
}

pub fn assert_safe_hostname(hostname: &str) -> Result<(), String> {
    let host = hostname.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return Err("url host is required".into());
    }
    if BLOCKED_HOSTNAMES.contains(&host.as_str()) || host.ends_with(".localhost") {
        return Err(format!("blocked hostname: {hostname}"));
    }
    if host.ends_with(".local") || host.ends_with(".internal") {
        return Err(format!("blocked special-use hostname: {hostname}"));
    }
    Ok(())
}

pub fn assert_safe_ip(ip: IpAddr) -> Result<(), String> {
    if is_blocked_ip(ip) {
        return Err(format!("blocked address: {ip}"));
    }
    Ok(())
}

pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_v4(v4),
        IpAddr::V6(v6) => is_blocked_v6(v6),
    }
}

fn is_blocked_v4(ip: Ipv4Addr) -> bool {
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_unspecified()
        || ip.is_multicast()
        || is_cgnat(ip)
        || ip.octets() == [169, 254, 169, 254] // cloud metadata
}

fn is_cgnat(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 100 && (octets[1] & 0xc0) == 64
}

fn is_blocked_v6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return true;
    }
    if ip.is_unique_local() || ip.is_unicast_link_local() {
        return true;
    }
    // IPv4-mapped
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_blocked_v4(v4);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_loopback_and_private_literals() {
        let _guard = lock_private_override(Some(false));
        assert!(assert_safe_http_url(&Url::parse("http://127.0.0.1/").unwrap()).is_err());
        assert!(assert_safe_http_url(&Url::parse("http://10.0.0.5/x").unwrap()).is_err());
        assert!(assert_safe_http_url(&Url::parse("http://192.168.1.1/").unwrap()).is_err());
        assert!(assert_safe_http_url(&Url::parse("http://169.254.169.254/latest").unwrap()).is_err());
        assert!(assert_safe_http_url(&Url::parse("http://[::1]/").unwrap()).is_err());
    }

    #[test]
    fn blocks_localhost_name() {
        let _guard = lock_private_override(Some(false));
        assert!(assert_safe_hostname("localhost").is_err());
        assert!(assert_safe_hostname("Foo.Localhost").is_err());
        assert!(assert_safe_hostname("metadata.google.internal").is_err());
    }

    #[test]
    fn allows_public_ip_literal() {
        let _guard = lock_private_override(Some(false));
        let url = Url::parse("https://8.8.8.8/").unwrap();
        assert!(assert_safe_http_url(&url).is_ok());
    }

    #[test]
    fn allow_private_escape_hatch() {
        let _guard = lock_private_override(Some(true));
        assert!(assert_safe_http_url(&Url::parse("http://127.0.0.1:9/").unwrap()).is_ok());
    }

    #[test]
    fn host_allowlist_matches_and_rejects() {
        let _guard = lock_host_allowlist(Some(vec!["mcp.example.com".into()]));
        let allowed = Url::parse("https://mcp.example.com/rpc").unwrap();
        let denied = Url::parse("https://evil.example.com/rpc").unwrap();
        let hosts = effective_host_allowlist("EVOHIME_MCP_ALLOWED_HOSTS").expect("override");
        assert!(assert_host_in_allowlist(&allowed, &hosts).is_ok());
        assert!(assert_host_in_allowlist(&denied, &hosts).is_err());
    }

    #[test]
    fn parse_host_allowlist_splits_and_normalizes() {
        assert_eq!(
            parse_host_allowlist(" MCP.Example.com. , localhost "),
            vec!["mcp.example.com".to_string(), "localhost".to_string()]
        );
    }
}
