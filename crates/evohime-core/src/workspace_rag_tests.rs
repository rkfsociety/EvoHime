use super::*;
use evohime_local_storage::LocalDatabase;

struct Fixture {
    root: PathBuf,
    database_path: PathBuf,
    database: LocalDatabase,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let suffix = format!("{}-{}", std::process::id(), now_ms());
        let root = std::env::temp_dir().join(format!("evohime-rag-{name}-{suffix}"));
        fs::create_dir_all(root.join("src")).unwrap();
        let database_path = std::env::temp_dir().join(format!("evohime-rag-{name}-{suffix}.db"));
        let database = LocalDatabase::open(&database_path).unwrap();
        Self {
            root,
            database_path,
            database,
        }
    }

    fn write(&self, relative: &str, content: &str) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn index(&mut self) -> IndexSummary {
        index_workspace(
            self.database.connection_mut(),
            &self.root,
            &IndexConfig::default(),
            false,
            || false,
            |_| {},
        )
        .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
        let _ = fs::remove_file(&self.database_path);
        let _ = fs::remove_file(self.database_path.with_extension("db-wal"));
        let _ = fs::remove_file(self.database_path.with_extension("db-shm"));
    }
}

#[test]
fn planner_is_deterministic_and_rejects_scope_escape() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../schemas/workspace-query-plan.schema.json")).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["confidence"]["maximum"], 1.0);
    let plan = plan_query(
        "UserAuthManager.validateToken",
        QueryFilters {
            path: None,
            language: None,
        },
    )
    .unwrap();
    assert_eq!(plan.strategy, QueryStrategy::ExactSymbol);
    assert_eq!(
        normalize_identifier("MyClass::method()", "java"),
        "myclass::method"
    );
    assert!(plan_query(
        "needle",
        QueryFilters {
            path: Some("../secret".into()),
            language: None
        },
    )
    .is_err());
    assert!(plan_query(
        "needle",
        QueryFilters {
            path: Some("src%".into()),
            language: None
        },
    )
    .is_err());
    let invalid = QueryPlan {
        need_search: true,
        strategy: QueryStrategy::ExactSymbol,
        query: String::new(),
        filters: QueryFilters {
            path: None,
            language: None,
        },
        reason: String::new(),
        confidence: 2.0,
    };
    let fallback = validated_plan_or_fallback(
        invalid,
        "find authentication handler",
        QueryFilters {
            path: None,
            language: None,
        },
    )
    .unwrap();
    assert_eq!(fallback.strategy, QueryStrategy::Lexical);
    assert_eq!(fallback.reason, "validation_failed");
    assert_eq!(fallback.confidence, 0.0);
    assert!(serde_json::from_str::<QueryPlan>(
            r#"{"need_search":true,"strategy":"lexical","query":"needle","filters":{"path":null,"language":null},"reason":"test","confidence":1.0,"unknown":true}"#
        )
        .is_err());
}

#[test]
fn loop_limits_use_fixed_priority_and_emit_a_terminal_event() {
    let mut fixture = Fixture::new("loop-limits");
    fixture.write("README.md", "bounded evidence");
    fixture.index();
    let result = search_workspace_with_config(
        fixture.database.connection(),
        &fixture.root,
        "bounded evidence",
        QueryFilters {
            path: None,
            language: None,
        },
        &RetrievalLimits::default(),
        &HybridConfig::default(),
        &LoopConfig {
            max_iterations: 0,
            wall_clock_timeout_ms: 0,
            token_budget: 0,
        },
    )
    .unwrap();
    assert_eq!(result.diagnostics.stop_reason, "iteration_limit");
    assert_eq!(
        result.diagnostics.reached_limits,
        vec!["iteration_limit", "timeout", "token_budget"]
    );
    assert_eq!(
        result
            .diagnostics
            .events
            .last()
            .map(|event| event.event_type.as_str()),
        Some("loop.stopped")
    );
    assert!(result
        .expansion_request
        .as_ref()
        .is_some_and(|request| request.requires_approval));
    let mut live_events = Vec::new();
    let _ = search_workspace_with_progress(SearchWorkspaceInput {
        connection: fixture.database.connection(),
        workspace_root: &fixture.root,
        query: "bounded evidence",
        filters: QueryFilters {
            path: None,
            language: None,
        },
        limits: &RetrievalLimits::default(),
        hybrid: &HybridConfig::default(),
        loop_config: &LoopConfig::default(),
        progress: |event: RetrievalProgress| live_events.push(event.event_type),
    })
    .unwrap();
    assert_eq!(
        live_events.first().map(String::as_str),
        Some("planner.started")
    );
    assert_eq!(live_events.last().map(String::as_str), Some("loop.stopped"));
}

#[test]
fn generation_publication_retrieval_and_incremental_reuse_work() {
    let mut fixture = Fixture::new("index-search");
    fixture.write(
        "src/auth.rs",
        "pub fn validate_token(value: &str) -> bool {\n    value == \"expected\"\n}\n",
    );
    fixture.write(
        "README.md",
        "# Authentication\nUse validate_token for checks.\n",
    );
    let first = fixture.index();
    assert_eq!(first.status, "published");
    assert_eq!(first.indexed_files, 2);
    let second = fixture.index();
    assert_eq!(second.reused_files, 2);
    let status = get_index_status(fixture.database.connection(), &fixture.root).unwrap();
    assert_eq!(status.generation, Some(2));
    let result = search_workspace(
        fixture.database.connection(),
        &fixture.root,
        "validate_token",
        QueryFilters {
            path: None,
            language: Some("rust".into()),
        },
        &RetrievalLimits::default(),
        &HybridConfig::default(),
    )
    .unwrap();
    assert!(!result.evidence.is_empty());
    assert!(result.evidence[0]
        .content
        .as_deref()
        .unwrap()
        .contains("validate_token"));
    assert_eq!(result.evidence[0].relative_path, "src/auth.rs");
    assert!(result.diagnostics.duration_ms < 500);

    let key = workspace_key(&fixture.root).unwrap();
    let generation = active_generation(fixture.database.connection(), &key)
        .unwrap()
        .unwrap();
    let details = fixture
            .database
            .connection()
            .prepare(
                "EXPLAIN QUERY PLAN SELECT document_id FROM workspace_documents
                 WHERE workspace_key = ?1 AND generation = ?2 AND language = 'rust' AND status = 'active'",
            )
            .unwrap()
            .query_map(params![key, generation], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join(" ");
    assert!(
        details.contains("INDEX"),
        "metadata scope must use an index: {details}"
    );
}

#[test]
fn cancelled_run_keeps_old_published_generation() {
    let mut fixture = Fixture::new("cancel");
    fixture.write("README.md", "# One\nold evidence\n");
    fixture.index();
    fixture.write("README.md", "# Two\nnew evidence\n");
    let mut progress_events = Vec::new();
    let result = index_workspace(
        fixture.database.connection_mut(),
        &fixture.root,
        &IndexConfig::default(),
        true,
        || true,
        |event| progress_events.push(event),
    );
    assert!(matches!(result, Err(RagError::Cancelled)));
    let status = get_index_status(fixture.database.connection(), &fixture.root).unwrap();
    assert_eq!(status.generation, Some(1));
    assert_eq!(status.status, "published");
    assert_eq!(
        progress_events.last().map(|event| event.phase.as_str()),
        Some("cancelled")
    );
}

#[test]
fn secret_binary_and_ragignore_paths_never_enter_index() {
    let mut fixture = Fixture::new("secrets");
    fixture.write("README.md", "public needle");
    fixture.write("notes.md", "client_secret = 'must-not-enter-index'");
    fixture.write(
        "tokens.txt",
        "ghp_0123456789012345678901234567890123456789\nAKIA0123456789ABCDEF",
    );
    fixture.write(".env", "API_KEY=super-secret");
    fixture.write("private.txt", "private marker");
    fixture.write(".ragignore", "private.txt\n");
    fs::write(fixture.root.join("binary.txt"), b"hello\0binary").unwrap();
    let summary = fixture.index();
    assert_eq!(summary.indexed_files, 1);
    let count: i64 = fixture
            .database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM workspace_documents WHERE path IN ('.env','private.txt','binary.txt','notes.md','tokens.txt')",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn vector_publication_is_atomic_and_hybrid_has_bounded_rrf_explanation() {
    let mut fixture = Fixture::new("hybrid");
    fixture.write(
        "src/search.rs",
        "pub fn search_workspace() { /* lexical vector fusion */ }",
    );
    fixture.write(
        "src/cache.rs",
        "pub fn cache_entries() { /* unrelated cache */ }",
    );
    fixture.write(
        "README.md",
        "# Guide\nUnrelated desktop setup instructions.",
    );
    fixture.index();
    let config = HybridConfig {
        enabled: true,
        ..HybridConfig::default()
    };
    let index_id = build_vector_index(
        fixture.database.connection_mut(),
        &fixture.root,
        &config,
        || false,
    )
    .unwrap();
    assert!(index_id.is_some());
    let result = search_workspace(
        fixture.database.connection(),
        &fixture.root,
        "search_workspace",
        QueryFilters {
            path: None,
            language: None,
        },
        &RetrievalLimits::default(),
        &config,
    )
    .unwrap();
    assert_eq!(result.diagnostics.mode, "hybrid");
    assert_eq!(result.evidence[0].ranking_explanation.algorithm, "rrf");
    assert!(result.evidence[0].ranking_explanation.sources.len() <= 2);
    let vector_only = search_workspace(
        fixture.database.connection(),
        &fixture.root,
        "concept_without_lexical_match",
        QueryFilters {
            path: None,
            language: None,
        },
        &RetrievalLimits::default(),
        &config,
    )
    .unwrap();
    assert_eq!(vector_only.diagnostics.mode, "hybrid");
    assert!(!vector_only.evidence.is_empty());
    assert!(vector_only.evidence.iter().any(|item| {
        item.ranking_explanation.lexical_rank.is_none()
            && item.ranking_explanation.vector_rank.is_some()
    }));
    for _ in 0..24 {
        let lexical = search_workspace(
            fixture.database.connection(),
            &fixture.root,
            "search_workspace",
            QueryFilters {
                path: None,
                language: None,
            },
            &RetrievalLimits::default(),
            &HybridConfig::default(),
        )
        .unwrap();
        let hybrid_result = search_workspace(
            fixture.database.connection(),
            &fixture.root,
            "search_workspace",
            QueryFilters {
                path: None,
                language: None,
            },
            &RetrievalLimits::default(),
            &config,
        )
        .unwrap();
        assert_eq!(lexical.evidence[0].relative_path, "src/search.rs");
        assert_eq!(hybrid_result.evidence[0].relative_path, "src/search.rs");
        let lexical_precision_at_3 = lexical
            .evidence
            .iter()
            .take(3)
            .filter(|item| item.relative_path == "src/search.rs")
            .count() as f64
            / 3.0;
        let hybrid_precision_at_3 = hybrid_result
            .evidence
            .iter()
            .take(3)
            .filter(|item| item.relative_path == "src/search.rs")
            .count() as f64
            / 3.0;
        let ndcg = |items: &[RetrievedChunk]| {
            items
                .iter()
                .take(3)
                .position(|item| item.relative_path == "src/search.rs")
                .map(|rank| 1.0 / ((rank + 2) as f64).log2())
                .unwrap_or(0.0)
        };
        assert!(hybrid_precision_at_3 >= lexical_precision_at_3);
        assert!(ndcg(&hybrid_result.evidence) >= ndcg(&lexical.evidence));
    }
}

#[test]
fn repeated_chunks_keep_their_persisted_byte_ranges() {
    let mut fixture = Fixture::new("repeated-range");
    fixture.write(
        "README.md",
        "# First\nrepeated evidence marker\n\n# Second\nrepeated evidence marker\n",
    );
    fixture.index();
    let result = search_workspace(
        fixture.database.connection(),
        &fixture.root,
        "repeated evidence marker",
        QueryFilters {
            path: None,
            language: None,
        },
        &RetrievalLimits::default(),
        &HybridConfig::default(),
    )
    .unwrap();
    let mut starts = result
        .evidence
        .iter()
        .filter_map(|item| item.lines.map(|lines| lines[0]))
        .collect::<Vec<_>>();
    starts.sort_unstable();
    starts.dedup();
    assert!(
        starts.len() >= 2,
        "duplicate text must retain distinct provenance"
    );
}

#[test]
fn context_ledger_contains_only_metadata_and_race_makes_citation_stale() {
    let mut fixture = Fixture::new("citation-race");
    fixture.write("README.md", "# Stable fact\nThe answer is forty two.\n");
    fixture.index();
    let search = search_workspace(
        fixture.database.connection(),
        &fixture.root,
        "forty two",
        QueryFilters {
            path: None,
            language: None,
        },
        &RetrievalLimits::default(),
        &HybridConfig::default(),
    )
    .unwrap();
    let context = build_evidence_context(
        fixture.database.connection(),
        &fixture.root,
        &search,
        4096,
        8,
        16,
    )
    .unwrap();
    assert!(context.model_context.contains("[cite:"));
    assert!(verify_document_provenance(
        fixture.database.connection(),
        &fixture.root,
        &search.evidence[0].relative_path,
        &search.evidence[0].chunk_hash,
    )
    .unwrap());
    let stored: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT snippet_hash || ':' || chunk_hash || ':' || path FROM rag_context_ledger LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert!(!stored.contains("forty two"));
    fixture.write(
        "README.md",
        "# Changed\nCompletely unrelated replacement.\n",
    );
    assert!(!verify_document_provenance(
        fixture.database.connection(),
        &fixture.root,
        &search.evidence[0].relative_path,
        &search.evidence[0].chunk_hash,
    )
    .unwrap());
    let final_context = finalize_citations(
        fixture.database.connection(),
        &fixture.root,
        &search,
        context,
    )
    .unwrap();
    assert!(final_context
        .citations
        .iter()
        .all(|citation| citation.status == CitationStatus::Stale));
    assert!(!final_context.model_context.contains("forty two"));
}

#[test]
fn utf16_is_indexed_without_hidden_normalization_and_invalid_limits_fail_closed() {
    let mut fixture = Fixture::new("encoding");
    let mut bytes = vec![0xff, 0xfe];
    for word in "# UTF16\nneedle\n".encode_utf16() {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    fs::write(fixture.root.join("README.md"), bytes).unwrap();
    fixture.index();
    let encoding: String = fixture
        .database
        .connection()
        .query_row("SELECT encoding FROM workspace_documents", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(encoding, "utf-16le");
    let bad = RetrievalLimits {
        max_context_chunks: 20,
        max_evidence_chunks: 10,
        ..RetrievalLimits::default()
    };
    assert!(bad.validate().is_err());
}
