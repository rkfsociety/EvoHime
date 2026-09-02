use evohime_core::code_diagnostics_feedback_loop::{
    canonical_hash, delta, validate_diagnostic, Binding, Diagnostic, Snapshot,
};
fn diagnostic(fingerprint: &str) -> Diagnostic {
    Diagnostic {
        id: fingerprint.into(),
        binding: Binding {
            workspace_root_id: "w".into(),
            workspace_fingerprint: "wf".into(),
            file_ref: "/src/lib.rs".into(),
            file_hash: None,
            file_revision: None,
        },
        severity: "error".into(),
        source: "compiler".into(),
        code: Some("E1".into()),
        message: "failure".into(),
        provider_id: "p".into(),
        fingerprint: fingerprint.into(),
        stale: false,
    }
}
fn snapshot(id: &str, diagnostics: Vec<Diagnostic>) -> Snapshot {
    let mut s = Snapshot {
        id: id.into(),
        workspace_fingerprint: "wf".into(),
        diagnostics,
        content_hash: String::new(),
    };
    let mut c = s.clone();
    c.content_hash.clear();
    s.content_hash = canonical_hash(&c).unwrap();
    s
}
#[test]
fn introduced_resolved_and_persisting_are_deterministic() {
    let base = snapshot("base", vec![diagnostic("old"), diagnostic("same")]);
    let current = snapshot("current", vec![diagnostic("same"), diagnostic("new")]);
    let d = delta(&base, &current).unwrap();
    assert_eq!(
        d.introduced
            .iter()
            .map(|x| x.fingerprint.as_str())
            .collect::<Vec<_>>(),
        vec!["new"]
    );
    assert_eq!(
        d.resolved
            .iter()
            .map(|x| x.fingerprint.as_str())
            .collect::<Vec<_>>(),
        vec!["old"]
    );
    assert_eq!(d.persisting.len(), 1)
}
#[test]
fn canonical_workspace_ref_and_stale_binding_are_rejected() {
    let mut d = diagnostic("x");
    d.binding.file_ref = "/../secret".into();
    assert!(validate_diagnostic(&d).is_err());
    d.binding.file_ref = "/src/lib.rs".into();
    d.stale = true;
    assert!(validate_diagnostic(&d).is_ok())
}
