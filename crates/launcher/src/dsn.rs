/// Builds a PostgreSQL connection string from stored components.
///
/// `initdb` on Windows creates a superuser named after the current Windows
/// user (not `postgres`), so the DSN must always be assembled dynamically
/// from `config.json` — never hardcoded as `postgres:...@...` (раздел III
/// плана). Percent-encodes each component so usernames containing
/// non-ASCII characters (e.g. Cyrillic Windows usernames) round-trip
/// correctly through libpq's URI parser.
pub fn build_dsn(user: &str, password: &str, host: &str, port: u16, db_name: &str) -> String {
    format!(
        "postgres://{}:{}@{}:{}/{}",
        percent_encode(user),
        percent_encode(password),
        host,
        port,
        percent_encode(db_name)
    )
}

/// Minimal percent-encoding of the unreserved character set (RFC 3986),
/// applied to the UTF-8 byte representation of `input` — this is exactly
/// how percent-encoding is meant to handle non-ASCII text.
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_expected_dsn_shape() {
        let dsn = build_dsn("evohime", "s3cr3t", "127.0.0.1", 5432, "evohime");
        assert_eq!(dsn, "postgres://evohime:s3cr3t@127.0.0.1:5432/evohime");
    }

    #[test]
    fn percent_encodes_non_ascii_username() {
        // initdb on a Windows machine with a Cyrillic username produces
        // exactly this kind of superuser name.
        let dsn = build_dsn("Роман", "pw", "127.0.0.1", 5432, "evohime");
        assert!(dsn.starts_with("postgres://%D0%A0%D0%BE%D0%BC%D0%B0%D0%BD:"));
    }

    #[test]
    fn percent_encodes_special_characters_in_password() {
        let encoded = percent_encode("p@ss/word?");
        assert_eq!(encoded, "p%40ss%2Fword%3F");
    }
}
