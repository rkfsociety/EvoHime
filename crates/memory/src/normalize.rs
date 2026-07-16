/// Trim and collapse internal whitespace. Preserves original casing for storage.
pub fn normalize_content(input: &str) -> String {
    input
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn collapses_newlines_and_tabs() {
        assert_eq!(normalize_content("a\n\tb"), "a b");
    }
}
