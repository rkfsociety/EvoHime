//! Small, case-insensitive glob matcher used by permission policy rules.

/// Match `*` and `?` against a value.
///
/// Unlike filesystem globs, `*` also matches `/`; permission subjects are
/// normalized Windows paths and command strings, not directory listings.
pub fn glob_match(pattern: &str, value: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let value: Vec<char> = value.to_lowercase().chars().collect();
    let (mut p, mut v) = (0usize, 0usize);
    let (mut star, mut retry) = (None, 0usize);

    while v < value.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == value[v]) {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            retry = v;
            p += 1;
        } else if let Some(star_pos) = star {
            p = star_pos + 1;
            retry += 1;
            v = retry;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::glob_match;

    #[test]
    fn matches_permission_subject_patterns() {
        assert!(glob_match("rm *", "rm -rf target"));
        assert!(glob_match("git *", "git status"));
        assert!(glob_match("git push*", "git push origin main"));
        assert!(glob_match("cargo *", "cargo publish"));
        assert!(glob_match("*.env", ".env"));
        assert!(glob_match("*.env", "backend/.env"));
        assert!(glob_match("*.env.*", ".env.local"));
        assert!(!glob_match("*.env", ".env.local"));
        assert!(!glob_match("*.env.*", "src/environment.rs"));
        assert!(glob_match("GIT ?USH", "git push"));
        assert!(glob_match("", ""));
        assert!(!glob_match("", "x"));
    }
}
