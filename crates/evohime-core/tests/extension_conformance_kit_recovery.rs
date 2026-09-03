use evohime_core::extension_conformance_kit::*;
#[test]
fn transactional_faults_leave_no_committed_instances() {
    let mut t = RegistrationTransaction::default();
    let mut d = ExtensionDescriptor {
        schema_version: 1,
        subject_id: "s".into(),
        kind: ExtensionKind::Workbench,
        provider_id: "p".into(),
        instance_id: "i".into(),
        api_version: 1,
        capability_refs: vec![],
        disabled: true,
        descriptor_hash: String::new(),
    };
    d.descriptor_hash = descriptor_hash(&d);
    t.stage(d).unwrap();
    assert_eq!(
        t.commit(FaultMode::BeforeCommit),
        Err(ConformanceError::RolledBack)
    );
}
#[test]
fn disabled_probe_with_effect_is_rejected() {
    let mut d = ExtensionDescriptor {
        schema_version: 1,
        subject_id: "s".into(),
        kind: ExtensionKind::Workbench,
        provider_id: "p".into(),
        instance_id: "i".into(),
        api_version: 1,
        capability_refs: vec![],
        disabled: true,
        descriptor_hash: String::new(),
    };
    d.descriptor_hash = descriptor_hash(&d);
    let p = ConformanceProbe {
        api_version: 1,
        instance_id: "i".into(),
        specialized_checks: [("workbench_contract".into(), true)].into_iter().collect(),
        side_effect_count: 0,
        disabled_side_effect_count: 1,
        security_assertions: [("safe".into(), true)].into_iter().collect(),
    };
    assert!(run(&d, &p, FaultMode::None).is_err());
}
