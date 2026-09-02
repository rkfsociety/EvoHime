use evohime_core::project_instruction_stack::*;
use std::fs;

#[test]
fn discovers_allowlisted_agents_and_compiles_path_specific_projection() {
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("crates");
    fs::create_dir_all(nested.join("generated")).unwrap();
    fs::write(root.path().join("AGENTS.md"), "# root\nUse tests\n").unwrap();
    fs::write(nested.join("AGENTS.md"), "---\nid: rust-rule\npaths:\n  - crates/**\nexclude_paths:\n  - crates/generated/**\nactivation: relevant-path\npriority: 10\n---\nNo unsafe Rust\n").unwrap();
    fs::write(root.path().join("README.md"), "not an instruction").unwrap();
    let rules = discover_rules(root.path(), None).unwrap();
    assert_eq!(rules.len(), 2);
    let snapshot = compile_snapshot(
        root.path(),
        rules,
        &["crates/core/lib.rs".into()],
        &[],
        &default_policy(),
        1,
    )
    .unwrap();
    assert_eq!(
        snapshot
            .active_rules
            .iter()
            .map(|rule| rule.id.as_str())
            .collect::<Vec<_>>(),
        vec!["rust-rule", "AGENTS.md"]
    );
    assert!(!project_snapshot(&snapshot)
        .to_string()
        .contains("No unsafe Rust"));
}

#[test]
fn budget_and_unknown_metadata_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(".evohime/rules");
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("bad.md"), "---\ntools: shell\n---\nRule").unwrap();
    assert_eq!(
        discover_rules(root.path(), None),
        Err(InstructionError::AuthorityMetadata)
    );
    let mut policy = default_policy();
    policy.max_total_tokens = MAX_TOKENS + 1;
    assert_eq!(
        validate_policy(&policy),
        Err(InstructionError::BudgetExceeded)
    );
}
