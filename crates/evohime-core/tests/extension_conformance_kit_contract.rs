use evohime_core::extension_conformance_kit::*;

fn descriptor(kind: ExtensionKind, disabled: bool) -> ExtensionDescriptor {
    let mut d = ExtensionDescriptor {
        schema_version: 1,
        subject_id: "subject".into(),
        kind,
        provider_id: "provider".into(),
        instance_id: "instance".into(),
        api_version: 1,
        capability_refs: vec![],
        disabled,
        descriptor_hash: String::new(),
    };
    d.descriptor_hash = descriptor_hash(&d);
    d
}
fn probe(kind: &ExtensionKind) -> ConformanceProbe {
    let key = match kind {
        ExtensionKind::IntegrationProvider => "integration_provider_contract",
        ExtensionKind::ExternalAgentAdapter => "external_agent_adapter_contract",
        ExtensionKind::Workbench => "workbench_contract",
        ExtensionKind::UiExtension => "ui_extension_contract",
        ExtensionKind::DeclarativeComponentProvider => "declarative_component_provider_contract",
    };
    ConformanceProbe {
        api_version: 1,
        instance_id: "instance".into(),
        specialized_checks: [(key.into(), true)].into_iter().collect(),
        side_effect_count: 0,
        disabled_side_effect_count: 0,
        security_assertions: [
            ("no_credentials".into(), true),
            ("no_authority_escalation".into(), true),
        ]
        .into_iter()
        .collect(),
    }
}
#[test]
fn all_required_specialized_suites_are_machine_checked() {
    for kind in [
        ExtensionKind::IntegrationProvider,
        ExtensionKind::ExternalAgentAdapter,
        ExtensionKind::Workbench,
    ] {
        let report = run(
            &descriptor(kind.clone(), false),
            &probe(&kind),
            FaultMode::None,
        )
        .unwrap();
        assert!(report.passed);
        assert_eq!(report.report_hash, report_hash(&report));
    }
}
#[test]
fn unsupported_api_is_fail_closed() {
    let mut d = descriptor(ExtensionKind::Workbench, false);
    d.api_version = 0;
    d.descriptor_hash = descriptor_hash(&d);
    assert!(validate_descriptor(&d).is_err());
}
