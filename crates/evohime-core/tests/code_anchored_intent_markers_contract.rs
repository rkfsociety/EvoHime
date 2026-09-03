use evohime_core::code_anchored_intent_markers as markers;

#[test]
fn marker_is_bound_to_revision_and_range() {
    let ranges = vec![markers::CommentRange {
        start_line: 7,
        end_line: 7,
        text: "// EVA? why debounce".into(),
    }];
    let found = markers::parse_comment_ranges(
        "src/app.ts",
        "abc123",
        &ranges,
        markers::Provenance::UserTrusted,
    )
    .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].range_start, 7);
    assert_eq!(found[0].revision, "abc123");
}

#[test]
fn existing_and_agent_content_never_auto_triggers() {
    for provenance in [
        markers::Provenance::ExistingRepository,
        markers::Provenance::AgentGenerated,
        markers::Provenance::ImportedUntrusted,
    ] {
        let found = markers::parse_comment_ranges(
            "src/app.ts",
            "abc123",
            &[markers::CommentRange {
                start_line: 1,
                end_line: 1,
                text: "// EVA! change".into(),
            }],
            provenance,
        )
        .unwrap();
        assert_eq!(
            markers::can_auto_propose(&found[0]),
            Err(markers::MarkerError::Untrusted)
        );
    }
}
