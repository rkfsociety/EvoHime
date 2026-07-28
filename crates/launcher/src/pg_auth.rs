use rand::distributions::Alphanumeric;
use rand::Rng;
use std::path::Path;
use tokio::fs;

#[derive(Debug, thiserror::Error)]
pub enum PgHbaError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Generates a random alphanumeric password for the PostgreSQL superuser.
///
/// `initdb` on Windows creates a superuser named after the current Windows
/// user (not `postgres`) with no known password — TCP connections would
/// otherwise require a password nobody has (раздел III плана). The Installer
/// calls this once at setup time and persists the result in `config.json`.
pub fn generate_password(length: usize) -> String {
    let mut rng = rand::thread_rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric) as char)
        .take(length)
        .collect()
}

/// Rewrites `pg_hba.conf` so that connections from `127.0.0.1/32` and
/// `::1/128` use the `trust` auth method instead of `md5`/`scram-sha-256`.
///
/// Safe here because the portable PostgreSQL cluster only ever accepts
/// connections from the same machine, under the same OS user account that
/// manages it — there is no multi-tenant exposure to protect against, and it
/// removes the risk of the stored password in `config.json` drifting out of
/// sync with the cluster's actual auth state (раздел III плана).
pub async fn patch_pg_hba_trust_local(pg_hba_path: &Path) -> Result<(), PgHbaError> {
    let contents = fs::read_to_string(pg_hba_path).await?;
    let patched = contents
        .lines()
        .map(rewrite_line_if_local_host)
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(pg_hba_path, patched).await?;
    Ok(())
}

fn rewrite_line_if_local_host(line: &str) -> String {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') || trimmed.is_empty() {
        return line.to_string();
    }
    let is_local_host = trimmed.contains("127.0.0.1/32") || trimmed.contains("::1/128");
    if !is_local_host {
        return line.to_string();
    }
    rewrite_auth_method_to_trust(line)
}

/// `pg_hba.conf` columns are whitespace-separated:
/// `TYPE  DATABASE  USER  ADDRESS  METHOD  [OPTIONS]`. The auth method is
/// always the 5th column for host-type entries.
fn rewrite_auth_method_to_trust(line: &str) -> String {
    let mut cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 5 {
        return line.to_string();
    }
    let method_idx = 4;
    cols[method_idx] = "trust";
    cols.join("\t")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_password_of_requested_length() {
        let pw = generate_password(24);
        assert_eq!(pw.len(), 24);
        assert!(pw.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn rewrites_only_local_host_lines() {
        let line_local = "host\tall\tall\t127.0.0.1/32\tscram-sha-256";
        let line_remote = "host\tall\tall\t0.0.0.0/0\tscram-sha-256";
        let comment = "# a comment line 127.0.0.1/32";

        assert_eq!(
            rewrite_line_if_local_host(line_local),
            "host\tall\tall\t127.0.0.1/32\ttrust"
        );
        assert_eq!(rewrite_line_if_local_host(line_remote), line_remote);
        assert_eq!(rewrite_line_if_local_host(comment), comment);
    }

    #[test]
    fn rewrites_ipv6_localhost() {
        let line = "host\tall\tall\t::1/128\tmd5";
        assert_eq!(
            rewrite_line_if_local_host(line),
            "host\tall\tall\t::1/128\ttrust"
        );
    }
}
