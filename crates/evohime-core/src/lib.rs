pub struct CoreVersion;

impl CoreVersion {
    pub const fn current() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

#[cfg(test)]
mod tests {
    use super::CoreVersion;

    #[test]
    fn core_exposes_version() {
        assert!(!CoreVersion::current().is_empty());
    }
}
