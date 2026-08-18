//! Integration tests for typed child contracts (03-1 plan).

use evohime_core::child_contracts::{
    ChildBudget, ContractError, ContractVersion, CorrelationContext, CorrelationId, Grant,
    Provenance, Schema, TypedChildReport, TypedChildTaskRequest, TypedReportStatus,
    accept_typed_report, validate_budget_subset, validate_grant_subset, CONTRACT_VERSION,
};

fn create_child_correlation(parent_sequence: u64) -> CorrelationContext {
    CorrelationContext::new(
        CorrelationId::new("parent-task-123").unwrap(),
        CorrelationId::new("child-task-456").unwrap(),
        parent_sequence,
    )
}

#[test]
fn test_full_child_workflow() {
    let correlation = create_child_correlation(1);
    
    let request = TypedChildTaskRequest::new(
        "child-task-456",
        "parent-task-123",
        "researcher",
        "analyze codebase",
        correlation.clone(),
    )
    .unwrap()
    .with_context(vec!["Inspect src/".to_string()])
    .unwrap()
    .with_max_output_bytes(8192)
    .unwrap()
    .with_capabilities(vec!["workspace.read".to_string()])
    .unwrap()
    .with_grants(vec![Grant::new("workspace.read").unwrap()])
    .unwrap()
    .with_budget(ChildBudget::new().with_tokens(5000).with_time(1800));

    assert!(request.validate().is_ok());

    let provenance = Provenance::new(1)
        .with_input_hash(Provenance::compute_hash("test input"))
        .unwrap()
        .with_model_id("gpt-4o".to_string())
        .unwrap();

    let report = TypedChildReport::new(
        "child-task-456",
        "parent-task-123",
        correlation,
        provenance,
    )
    .unwrap()
    .with_status(TypedReportStatus::Complete)
    .with_summary("Found 5 modules".to_string())
    .unwrap()
    .with_findings(vec!["Module A".to_string()])
    .unwrap()
    .with_sources(vec!["src/lib.rs:1".to_string()])
    .unwrap()
    .with_confidence(95);

    assert!(report.validate().is_ok());
    assert!(report.validate_against_request(&request).is_ok());
    
    let accepted = accept_typed_report(&request, &report).unwrap();
    assert_eq!(accepted.child_task_id, "child-task-456");
}

#[test]
fn test_grant_subset_validation() {
    let parent_grants = vec![
        Grant::new("workspace.read").unwrap(),
        Grant::new("git.status").unwrap(),
    ];

    let child_grants = vec![Grant::new("workspace.read").unwrap()];
    assert!(validate_grant_subset(&child_grants, &parent_grants).is_ok());

    let escalated = vec![Grant::new("workspace.write").unwrap()];
    assert!(matches!(
        validate_grant_subset(&escalated, &parent_grants),
        Err(ContractError::GrantEscalation { .. })
    ));
}

#[test]
fn test_budget_validation() {
    let parent = ChildBudget::new().with_tokens(1000).with_time(60);
    let child = ChildBudget::new().with_tokens(500).with_time(30);
    assert!(validate_budget_subset(&Some(child), &Some(parent.clone())).is_ok());

    let oversized = ChildBudget::new().with_tokens(1500);
    assert!(matches!(
        validate_budget_subset(&Some(oversized), &Some(parent.clone())),
        Err(ContractError::BudgetExceedsParent)
    ));
}

#[test]
fn test_correlation_tracking() {
    let correlation = CorrelationContext::new(
        CorrelationId::new("task-1").unwrap(),
        CorrelationId::new("child-1").unwrap(),
        1,
    )
    .with_tool_call(CorrelationId::new("tool-1").unwrap())
    .with_receipt(CorrelationId::new("receipt-1").unwrap());

    assert!(correlation.validate().is_ok());
    assert!(correlation.tool_call_id.is_some());
    assert!(correlation.receipt_id.is_some());
}

#[test]
fn test_contract_versioning() {
    assert_eq!(CONTRACT_VERSION, ContractVersion::new(1, 0));
    
    let v1_0 = ContractVersion::new(1, 0);
    let v1_1 = ContractVersion::new(1, 1);
    assert!(v1_0.is_compatible_with(&v1_1));
    assert!(v1_0.can_accept_additive(&v1_1));
}

#[test]
fn test_provenance_hashing() {
    let hash = Provenance::compute_hash("test data");
    assert_eq!(hash.len(), 64);
    
    let provenance = Provenance::new(1)
        .with_input_hash(hash.clone()).unwrap()
        .with_model_id("model-1".to_string()).unwrap();
    
    assert_eq!(provenance.input_hash, Some(hash));
}

#[test]
fn test_nested_child_forbidden() {
    let mut request = TypedChildTaskRequest::new(
        "child-1", "task-1", "researcher", "test", create_child_correlation(0),
    )
    .unwrap();
    
    assert!(request.validate().is_ok());
    request.parent_is_child = true;
    assert!(matches!(request.validate(), Err(ContractError::NestedChildForbidden)));
}

#[test]
fn test_task_mismatch() {
    let request = TypedChildTaskRequest::new(
        "child-1", "task-1", "researcher", "test", create_child_correlation(0),
    )
    .unwrap();

    let report = TypedChildReport::new(
        "child-2", "task-1", create_child_correlation(0), Provenance::new(0),
    )
    .unwrap();

    assert!(matches!(
        report.validate_against_request(&request),
        Err(ContractError::TaskMismatch)
    ));
}

#[test]
fn test_schema_validation() {
    let schema = Schema::new().with_max_bytes(100);
    assert!(schema.validate_content("short").is_ok());
    assert!(schema.validate_content(&"x".repeat(101)).is_err());
}

#[test]
fn test_deterministic_serialization() {
    let request = TypedChildTaskRequest::new(
        "child-1", "task-1", "researcher", "test", create_child_correlation(0),
    )
    .unwrap();
    
    let json1 = request.to_deterministic_json().unwrap();
    let json2 = request.to_deterministic_json().unwrap();
    assert_eq!(json1, json2);
}
