use super::*;

pub fn stable_id(workspace_key: &str, generation: i64, value: &str, kind: &str) -> String {
    format!(
        "{kind}-{}",
        &sha256_hex(format!("{workspace_key}:{generation}:{kind}:{value}").as_bytes())[..32]
    )
}

pub fn bounded_error(path: &str, error: &str) -> String {
    let code = if error.to_lowercase().contains("permission") {
        "permission_denied"
    } else if error.to_lowercase().contains("not found") {
        "file_not_found"
    } else if error.to_lowercase().contains("timeout") {
        "timeout"
    } else {
        "io_error"
    };
    format!("{}:{code}", path.chars().take(256).collect::<String>())
}

pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4).max(1)
}

pub fn search_workspace(
    connection: &Connection,
    workspace_root: &Path,
    query: &str,
    filters: QueryFilters,
    limits: &RetrievalLimits,
    hybrid: &HybridConfig,
) -> Result<SearchResult, RagError> {
    search_workspace_with_config(
        connection,
        workspace_root,
        query,
        filters,
        limits,
        hybrid,
        &LoopConfig::default(),
    )
}

pub fn search_workspace_with_config(
    connection: &Connection,
    workspace_root: &Path,
    query: &str,
    filters: QueryFilters,
    limits: &RetrievalLimits,
    hybrid: &HybridConfig,
    loop_config: &LoopConfig,
) -> Result<SearchResult, RagError> {
    search_workspace_with_progress(SearchWorkspaceInput {
        connection,
        workspace_root,
        query,
        filters,
        limits,
        hybrid,
        loop_config,
        progress: |_| {},
    })
}

pub struct SearchWorkspaceInput<'a, F> {
    pub connection: &'a Connection,
    pub workspace_root: &'a Path,
    pub query: &'a str,
    pub filters: QueryFilters,
    pub limits: &'a RetrievalLimits,
    pub hybrid: &'a HybridConfig,
    pub loop_config: &'a LoopConfig,
    pub progress: F,
}

pub fn search_workspace_with_progress<F: FnMut(RetrievalProgress)>(
    input: SearchWorkspaceInput<'_, F>,
) -> Result<SearchResult, RagError> {
    let SearchWorkspaceInput {
        connection,
        workspace_root,
        query,
        filters,
        limits,
        hybrid,
        loop_config,
        mut progress,
    } = input;
    limits.validate()?;
    validate_filters(&filters)?;
    if loop_config.max_iterations > 2
        || loop_config.wall_clock_timeout_ms > 30_000
        || loop_config.token_budget > 128_000
    {
        return Err(RagError::InvalidConfig(
            "agentic loop limits exceed hard bounds".into(),
        ));
    }
    let started = Instant::now();
    let key = workspace_key(workspace_root)?;
    let generation = active_generation(connection, &key)?.ok_or(RagError::IndexUnavailable)?;
    let mut plan = plan_query(query, filters)?;
    let query_id = format!(
        "query-{}",
        &sha256_hex(format!("{}:{}:{}", key, generation, query).as_bytes())[..24]
    );
    let planner_event = RetrievalProgress {
        event_type: "planner.started".into(),
        iteration: 0,
        strategy: plan.strategy.clone(),
        result_count: 0,
        coverage_millis: 0,
        reason_code: "deterministic_precheck".into(),
    };
    progress(planner_event.clone());
    let mut events = vec![planner_event];
    if loop_config.max_iterations == 0
        || loop_config.wall_clock_timeout_ms == 0
        || loop_config.token_budget == 0
    {
        let mut reached_limits: Vec<String> = Vec::new();
        if loop_config.max_iterations == 0 {
            reached_limits.push("iteration_limit".into());
        }
        if loop_config.wall_clock_timeout_ms == 0 {
            reached_limits.push("timeout".into());
        }
        if loop_config.token_budget == 0 {
            reached_limits.push("token_budget".into());
        }
        let stop_reason = reached_limits[0].clone();
        push_progress(
            &mut events,
            &mut progress,
            RetrievalProgress {
                event_type: "loop.stopped".into(),
                iteration: 0,
                strategy: plan.strategy.clone(),
                result_count: 0,
                coverage_millis: 0,
                reason_code: stop_reason.clone(),
            },
        );
        let expansion_request = expansion_request(&plan.filters, "bounded_limit");
        return Ok(SearchResult {
            query_id,
            plan,
            evidence: Vec::new(),
            diagnostics: SearchDiagnostics {
                mode: "fts5".into(),
                fallback_reason: None,
                metrics_version: EVIDENCE_METRICS_VERSION.into(),
                iterations: 0,
                coverage: 0.0,
                stop_reason,
                result_count: 0,
                duration_ms: started.elapsed().as_millis() as u64,
                query_hash: sha256_hex(query.as_bytes()),
                conflict_flag: false,
                reached_limits,
                events,
            },
            uncertainty: Some("Данных workspace недостаточно: bounded loop не запущен".into()),
            expansion_request: Some(expansion_request),
        });
    }
    if !plan.need_search {
        push_progress(
            &mut events,
            &mut progress,
            RetrievalProgress {
                event_type: "loop.stopped".into(),
                iteration: 0,
                strategy: plan.strategy.clone(),
                result_count: 0,
                coverage_millis: 0,
                reason_code: "search_not_needed".into(),
            },
        );
        return Ok(SearchResult {
            query_id,
            plan,
            evidence: Vec::new(),
            diagnostics: SearchDiagnostics {
                mode: "fts5".into(),
                fallback_reason: None,
                metrics_version: EVIDENCE_METRICS_VERSION.into(),
                iterations: 0,
                coverage: 0.0,
                stop_reason: "search_not_needed".into(),
                result_count: 0,
                duration_ms: started.elapsed().as_millis() as u64,
                query_hash: sha256_hex(query.as_bytes()),
                conflict_flag: false,
                reached_limits: Vec::new(),
                events,
            },
            uncertainty: None,
            expansion_request: None,
        });
    }

    let mut seen = HashSet::new();
    let mut evidence = Vec::new();
    let mut iterations = 0usize;
    let mut stop_reason = "evidence_sufficient".to_string();
    let mut coverage = 0.0;
    let retrieval_token_budget = loop_config.token_budget.saturating_mul(60) / 100;
    let mut consumed_tokens = 0usize;
    let mut reached_limits = Vec::new();
    for _ in 0..loop_config.max_iterations {
        if started.elapsed() >= Duration::from_millis(loop_config.wall_clock_timeout_ms) {
            stop_reason = "timeout".into();
            reached_limits.push("timeout".into());
            break;
        }
        if consumed_tokens >= retrieval_token_budget {
            stop_reason = "token_budget".into();
            reached_limits.push("token_budget".into());
            break;
        }
        let fingerprint = format!("{:?}:{}:{:?}", plan.strategy, plan.query, plan.filters);
        if !seen.insert(fingerprint) {
            stop_reason = "duplicate_rewrite".into();
            break;
        }
        iterations += 1;
        let remaining = Duration::from_millis(loop_config.wall_clock_timeout_ms)
            .saturating_sub(started.elapsed());
        evidence = match bounded_lexical_retrieval(BoundedLexicalRetrievalInput {
            connection,
            workspace_root,
            workspace_key: &key,
            generation,
            plan: &plan,
            limits,
            timeout: remaining / 2,
        }) {
            Ok(evidence) => evidence,
            Err(RagError::Timeout)
                if started.elapsed() < Duration::from_millis(loop_config.wall_clock_timeout_ms) =>
            {
                let retry_remaining = Duration::from_millis(loop_config.wall_clock_timeout_ms)
                    .saturating_sub(started.elapsed());
                match bounded_lexical_retrieval(BoundedLexicalRetrievalInput {
                    connection,
                    workspace_root,
                    workspace_key: &key,
                    generation,
                    plan: &plan,
                    limits,
                    timeout: retry_remaining,
                }) {
                    Ok(evidence) => evidence,
                    Err(_) => {
                        return Ok(retrieval_failure_result(RetrievalFailureInput {
                            query_id,
                            plan,
                            query,
                            started,
                            events,
                            iterations,
                            reason: "retrieval_error",
                            progress: &mut progress,
                        }));
                    }
                }
            }
            Err(RagError::Sandbox(_)) => {
                return Ok(retrieval_failure_result(RetrievalFailureInput {
                    query_id,
                    plan,
                    query,
                    started,
                    events,
                    iterations,
                    reason: "security_rejected",
                    progress: &mut progress,
                }));
            }
            Err(_) => {
                return Ok(retrieval_failure_result(RetrievalFailureInput {
                    query_id,
                    plan,
                    query,
                    started,
                    events,
                    iterations,
                    reason: "retrieval_error",
                    progress: &mut progress,
                }));
            }
        };
        consumed_tokens = consumed_tokens.saturating_add(
            evidence
                .iter()
                .map(|item| estimate_tokens(item.content.as_deref().unwrap_or_default()))
                .sum::<usize>(),
        );
        coverage = checker_coverage(&plan, &evidence);
        for chunk in &mut evidence {
            chunk.checker_confidence = checker_confidence(&plan, chunk, coverage);
        }
        push_progress(
            &mut events,
            &mut progress,
            RetrievalProgress {
                event_type: "retrieval.updated".into(),
                iteration: iterations,
                strategy: plan.strategy.clone(),
                result_count: evidence.len(),
                coverage_millis: (coverage.clamp(0.0, 1.0) * 1000.0).round() as u16,
                reason_code: if evidence.is_empty() {
                    "empty_result"
                } else {
                    "retrieved"
                }
                .into(),
            },
        );
        push_progress(
            &mut events,
            &mut progress,
            RetrievalProgress {
                event_type: "checker.updated".into(),
                iteration: iterations,
                strategy: plan.strategy.clone(),
                result_count: evidence.len(),
                coverage_millis: (coverage.clamp(0.0, 1.0) * 1000.0).round() as u16,
                reason_code: if coverage >= 0.8 {
                    "sufficient"
                } else {
                    "low_coverage"
                }
                .into(),
            },
        );
        if evidence.is_empty() {
            stop_reason = "empty_result".into();
        } else if coverage >= 0.8 {
            break;
        } else {
            stop_reason = "low_coverage".into();
        }
        if iterations < loop_config.max_iterations {
            push_progress(
                &mut events,
                &mut progress,
                RetrievalProgress {
                    event_type: "rewrite.started".into(),
                    iteration: iterations + 1,
                    strategy: plan.strategy.clone(),
                    result_count: evidence.len(),
                    coverage_millis: (coverage.clamp(0.0, 1.0) * 1000.0).round() as u16,
                    reason_code: stop_reason.clone(),
                },
            );
            plan = rewrite_plan(&plan);
        }
    }

    let mut mode = "fts5".to_string();
    let mut fallback_reason = None;
    if hybrid.enabled && hybrid_allowed(&plan, hybrid) {
        match hybrid_retrieval(HybridRetrievalInput {
            connection,
            workspace_root,
            workspace_key: &key,
            generation,
            query,
            filters: &plan.filters,
            lexical: &evidence,
            limits,
        }) {
            Ok(Some(fused)) => {
                evidence = fused;
                mode = "hybrid".into();
            }
            Ok(None) => {
                mode = "fallback_fts5".into();
                fallback_reason = Some("vector_index_unavailable".into());
            }
            Err(_) => {
                mode = "fallback_fts5".into();
                fallback_reason = Some("vector_index_incompatible".into());
            }
        }
    }
    evidence.truncate(limits.max_evidence_chunks);
    let conflict_flag = unresolved_conflict(&evidence);
    if iterations >= loop_config.max_iterations && coverage < 0.8 {
        stop_reason = "iteration_limit".into();
        reached_limits.insert(0, "iteration_limit".into());
    }
    let uncertainty = if conflict_flag {
        stop_reason = "conflict_unresolved".into();
        Some("Источники workspace противоречат друг другу; конфликт нельзя разрешить молча".into())
    } else if evidence.is_empty() || coverage < 0.8 {
        Some("Данных workspace недостаточно для подтверждённого ответа".into())
    } else {
        None
    };
    let result_count = evidence.len();
    let expansion_request = uncertainty
        .as_ref()
        .map(|_| expansion_request(&plan.filters, &stop_reason));
    if expansion_request.is_some() {
        push_progress(
            &mut events,
            &mut progress,
            RetrievalProgress {
                event_type: "expansion.requested".into(),
                iteration: iterations,
                strategy: plan.strategy.clone(),
                result_count,
                coverage_millis: (coverage.clamp(0.0, 1.0) * 1000.0).round() as u16,
                reason_code: stop_reason.clone(),
            },
        );
    }
    push_progress(
        &mut events,
        &mut progress,
        RetrievalProgress {
            event_type: "loop.stopped".into(),
            iteration: iterations,
            strategy: plan.strategy.clone(),
            result_count,
            coverage_millis: (coverage.clamp(0.0, 1.0) * 1000.0).round() as u16,
            reason_code: stop_reason.clone(),
        },
    );
    Ok(SearchResult {
        query_id,
        plan,
        evidence,
        diagnostics: SearchDiagnostics {
            mode,
            fallback_reason,
            metrics_version: EVIDENCE_METRICS_VERSION.into(),
            iterations,
            coverage,
            stop_reason,
            result_count,
            duration_ms: started.elapsed().as_millis() as u64,
            query_hash: sha256_hex(query.as_bytes()),
            conflict_flag,
            reached_limits,
            events,
        },
        uncertainty,
        expansion_request,
    })
}

fn expansion_request(filters: &QueryFilters, reason: &str) -> ExpansionRequest {
    ExpansionRequest {
        request_type: "request_expansion".into(),
        suggested_path: filters.path.clone().unwrap_or_else(|| "src".into()),
        languages: filters.language.clone().into_iter().collect(),
        reason: reason.chars().take(64).collect(),
        estimated_iterations: 1,
        estimated_tokens: 1200,
        estimated_seconds: 5,
        requires_approval: true,
    }
}

fn push_progress(
    events: &mut Vec<RetrievalProgress>,
    progress: &mut impl FnMut(RetrievalProgress),
    event: RetrievalProgress,
) {
    progress(event.clone());
    events.push(event);
}

struct RetrievalFailureInput<'a, F> {
    query_id: String,
    plan: QueryPlan,
    query: &'a str,
    started: Instant,
    events: Vec<RetrievalProgress>,
    iterations: usize,
    reason: &'a str,
    progress: &'a mut F,
}

fn retrieval_failure_result<F: FnMut(RetrievalProgress)>(
    input: RetrievalFailureInput<'_, F>,
) -> SearchResult {
    let RetrievalFailureInput {
        query_id,
        plan,
        query,
        started,
        mut events,
        iterations,
        reason,
        progress,
    } = input;
    push_progress(
        &mut events,
        progress,
        RetrievalProgress {
            event_type: "loop.stopped".into(),
            iteration: iterations,
            strategy: plan.strategy.clone(),
            result_count: 0,
            coverage_millis: 0,
            reason_code: reason.into(),
        },
    );
    let expansion = expansion_request(&plan.filters, reason);
    SearchResult {
        query_id,
        plan,
        evidence: Vec::new(),
        diagnostics: SearchDiagnostics {
            mode: "fts5".into(),
            fallback_reason: Some(reason.into()),
            metrics_version: EVIDENCE_METRICS_VERSION.into(),
            iterations,
            coverage: 0.0,
            stop_reason: reason.into(),
            result_count: 0,
            duration_ms: started.elapsed().as_millis() as u64,
            query_hash: sha256_hex(query.as_bytes()),
            conflict_flag: false,
            reached_limits: Vec::new(),
            events,
        },
        uncertainty: Some("Данные workspace недоступны; утверждение не подтверждено".into()),
        expansion_request: Some(expansion),
    }
}

fn unresolved_conflict(evidence: &[RetrievedChunk]) -> bool {
    evidence.iter().enumerate().any(|(index, left)| {
        evidence.iter().skip(index + 1).any(|right| {
            left.source_id != right.source_id
                && left.symbol.is_some()
                && left.symbol == right.symbol
                && left.content_hash != right.content_hash
                && left.content.as_deref() != right.content.as_deref()
        })
    })
}

struct BoundedLexicalRetrievalInput<'a> {
    connection: &'a Connection,
    workspace_root: &'a Path,
    workspace_key: &'a str,
    generation: i64,
    plan: &'a QueryPlan,
    limits: &'a RetrievalLimits,
    timeout: Duration,
}

fn bounded_lexical_retrieval(
    input: BoundedLexicalRetrievalInput<'_>,
) -> Result<Vec<RetrievedChunk>, RagError> {
    if input.timeout.is_zero() {
        return Err(RagError::Timeout);
    }
    let deadline = Instant::now() + input.timeout;
    input
        .connection
        .progress_handler(1_000, Some(move || Instant::now() >= deadline));
    let result = lexical_retrieval(
        input.connection,
        input.workspace_root,
        input.workspace_key,
        input.generation,
        input.plan,
        input.limits,
    );
    input.connection.progress_handler(0, None::<fn() -> bool>);
    match result {
        Err(RagError::Sqlite(error)) if error.to_string().to_lowercase().contains("interrupt") => {
            Err(RagError::Timeout)
        }
        other => other,
    }
}

fn lexical_retrieval(
    connection: &Connection,
    workspace_root: &Path,
    workspace_key: &str,
    generation: i64,
    plan: &QueryPlan,
    limits: &RetrievalLimits,
) -> Result<Vec<RetrievedChunk>, RagError> {
    let terms = bounded_terms(&plan.query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let normalized_terms = terms
        .iter()
        .map(|term| term.to_lowercase())
        .collect::<Vec<_>>();
    let match_expression = terms
        .iter()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ");
    let path_filter = plan.filters.path.as_deref().unwrap_or("");
    let language_filter = plan.filters.language.as_deref().unwrap_or("");
    let column_query = match plan.strategy {
        QueryStrategy::ExactSymbol => format!("symbol_normalized : ({match_expression})"),
        QueryStrategy::Path => format!("path : ({match_expression})"),
        _ => match_expression,
    };
    let mut statement = connection.prepare(
        "SELECT c.chunk_id, c.document_id, d.path, d.language, c.byte_start,
                c.byte_end, c.chunk_hash, d.file_hash, c.chunk_text, c.symbol,
                c.parent_context, d.redaction_status, d.size_bytes,
                bm25(workspace_chunks_fts, 1.0, 2.0, 0.5, 0.25) AS rank_score,
                c.ordinal
         FROM workspace_chunks_fts
         JOIN document_chunks c ON c.chunk_id = workspace_chunks_fts.chunk_id
         JOIN workspace_documents d ON d.document_id = c.document_id
         WHERE workspace_chunks_fts MATCH ?1
           AND d.workspace_key = ?2 AND d.generation = ?3 AND d.status = 'active'
           AND d.is_secret_path = 0
           AND (?4 = '' OR d.path = ?4 OR d.path LIKE ?4 || '/%')
           AND (?5 = '' OR d.language = ?5)
         ORDER BY rank_score ASC, d.path COLLATE BINARY ASC, d.document_id ASC,
                  c.ordinal ASC, c.byte_start ASC
         LIMIT ?6",
    )?;
    let candidates = statement
        .query_map(
            params![
                column_query,
                workspace_key,
                generation,
                path_filter,
                language_filter,
                limits.max_retrieval_chunks as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, i64>(12)?,
                    row.get::<_, f64>(13)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let canonical_root = workspace_root.canonicalize()?;
    let mut output = Vec::new();
    for (rank, candidate) in candidates.into_iter().enumerate() {
        let (
            chunk_id,
            document_id,
            path,
            language,
            byte_start,
            byte_end,
            chunk_hash,
            file_hash,
            indexed_content,
            symbol,
            parent_context,
            redaction,
            indexed_size,
            bm25,
        ) = candidate;
        let mut matched_filters = vec![format!("workspace_key={workspace_key}")];
        if !path_filter.is_empty() {
            matched_filters.push(format!("path={path_filter}"));
        }
        if !language_filter.is_empty() {
            matched_filters.push(format!("language={language_filter}"));
        }
        let score = -bm25;
        let current_path = canonical_root.join(&path);
        let validation = validate_source(
            &canonical_root,
            &current_path,
            &file_hash,
            indexed_size as u64,
            byte_start as usize,
            byte_end as usize,
        );
        let (content, lines, stale) = match validation {
            Ok(bytes) if redaction != "full" => {
                match decode_source_range(&bytes, byte_start as usize, byte_end as usize) {
                    Ok((content, lines)) if content.trim() == indexed_content.trim() => {
                        (Some(content.trim().to_string()), Some(lines), false)
                    }
                    _ => (None, None, true),
                }
            }
            Ok(_) => (None, None, false),
            Err(_) => (None, None, true),
        };
        let indexed_lower = indexed_content.to_lowercase();
        let term_frequencies = terms
            .iter()
            .zip(&normalized_terms)
            .map(|(term, normalized_term)| {
                let count = indexed_lower.matches(normalized_term).count();
                (term.clone(), count)
            })
            .collect::<BTreeMap<_, _>>();
        output.push(RetrievedChunk {
            source_id: document_id,
            chunk_id,
            relative_path: path,
            language,
            byte_start: byte_start as u64,
            byte_end: byte_end as u64,
            lines,
            chunk_hash,
            content_hash: file_hash,
            content,
            symbol,
            parent_context,
            score,
            score_explanation: ScoreExplanation {
                algorithm: "bm25".into(),
                column_weights: BTreeMap::from([
                    ("content".into(), 1.0),
                    ("symbol_normalized".into(), 2.0),
                    ("canonical_path".into(), 0.5),
                ]),
                term_frequencies,
                document_length: indexed_content.len(),
                matched_filters,
                excluded_by: Vec::new(),
            },
            ranking_explanation: RankingExplanation {
                algorithm: "fts5".into(),
                lexical_rank: Some(rank + 1),
                vector_rank: None,
                rrf_rank: rank + 1,
                sources: vec!["lexical".into()],
            },
            stale,
            redaction_status: redaction,
            checker_confidence: 0.0,
        });
    }
    output.sort_by(deterministic_rank);
    output.truncate(limits.max_retrieval_chunks);
    Ok(output)
}

pub fn previous_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

pub fn validate_source(
    root: &Path,
    path: &Path,
    expected_hash: &str,
    expected_size: u64,
    byte_start: usize,
    byte_end: usize,
) -> Result<Vec<u8>, RagError> {
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(root) {
        return Err(RagError::Sandbox("retrieval path escaped workspace".into()));
    }
    let bytes = fs::read(&canonical)?;
    if bytes.len() as u64 != expected_size
        || sha256_hex(&bytes) != expected_hash
        || byte_start > byte_end
        || byte_end > bytes.len()
    {
        return Err(RagError::InvalidWorkspace("stale source snapshot".into()));
    }
    Ok(bytes)
}

fn decode_source_range(
    bytes: &[u8],
    byte_start: usize,
    byte_end: usize,
) -> Result<(String, [u64; 2]), RagError> {
    if byte_start > byte_end || byte_end > bytes.len() {
        return Err(RagError::InvalidWorkspace(
            "invalid source byte range".into(),
        ));
    }
    let (content, prefix) = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff])
    {
        let little_endian = bytes.starts_with(&[0xff, 0xfe]);
        let decode = |slice: &[u8]| -> Result<String, RagError> {
            if !slice.len().is_multiple_of(2) {
                return Err(RagError::InvalidWorkspace(
                    "unaligned UTF-16 byte range".into(),
                ));
            }
            let units = slice
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| {
                    if little_endian {
                        u16::from_le_bytes(*pair)
                    } else {
                        u16::from_be_bytes(*pair)
                    }
                })
                .collect::<Vec<_>>();
            String::from_utf16(&units)
                .map_err(|_| RagError::InvalidWorkspace("invalid UTF-16 source range".into()))
        };
        let start = byte_start.max(2);
        let end = byte_end.max(2);
        (decode(&bytes[start..end])?, decode(&bytes[2..start])?)
    } else {
        (
            String::from_utf8_lossy(&bytes[byte_start..byte_end]).into_owned(),
            String::from_utf8_lossy(&bytes[..byte_start]).into_owned(),
        )
    };
    let start_line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u64 + 1;
    let end_line = start_line + content.bytes().filter(|byte| *byte == b'\n').count() as u64;
    Ok((content, [start_line, end_line]))
}

fn deterministic_rank(left: &RetrievedChunk, right: &RetrievedChunk) -> Ordering {
    let score = if (left.score - right.score).abs() <= 1e-9 {
        Ordering::Equal
    } else {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
    };
    score
        .then_with(|| {
            left.relative_path
                .as_bytes()
                .cmp(right.relative_path.as_bytes())
        })
        .then_with(|| left.source_id.cmp(&right.source_id))
        .then_with(|| left.byte_start.cmp(&right.byte_start))
        .then_with(|| left.chunk_id.cmp(&right.chunk_id))
}

fn checker_coverage(plan: &QueryPlan, evidence: &[RetrievedChunk]) -> f64 {
    if evidence.is_empty() {
        return 0.0;
    }
    match plan.strategy {
        QueryStrategy::ExactSymbol => evidence.iter().any(|item| {
            item.symbol.as_ref().is_some_and(|symbol| {
                normalize_identifier(symbol, &item.language)
                    .contains(&normalize_identifier(&plan.query, &item.language))
            })
        }) as u8 as f64,
        QueryStrategy::Path => {
            let matching = evidence
                .iter()
                .filter(|item| {
                    item.relative_path
                        .to_lowercase()
                        .contains(&plan.query.to_lowercase())
                })
                .count();
            matching as f64 / evidence.len() as f64
        }
        QueryStrategy::Metadata => evidence.iter().any(|item| {
            plan.filters
                .language
                .as_ref()
                .is_none_or(|language| &item.language == language)
                && plan
                    .filters
                    .path
                    .as_ref()
                    .is_none_or(|path| item.relative_path.starts_with(path))
        }) as u8 as f64,
        QueryStrategy::Lexical => {
            let terms = bounded_terms(&plan.query);
            let combined = evidence
                .iter()
                .filter_map(|item| item.content.as_deref())
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            let matched = terms
                .iter()
                .filter(|term| combined.contains(&term.to_lowercase()))
                .count();
            matched as f64 / terms.len().max(1) as f64
        }
    }
}

fn checker_confidence(plan: &QueryPlan, chunk: &RetrievedChunk, coverage: f64) -> f64 {
    if chunk.stale || chunk.redaction_status == "full" || chunk.content.is_none() {
        return 0.0;
    }
    let score_gate = if chunk.score.is_finite() { 1.0 } else { 0.0 };
    let strategy_gate = match plan.strategy {
        QueryStrategy::Lexical => coverage,
        QueryStrategy::ExactSymbol => chunk.symbol.is_some() as u8 as f64,
        QueryStrategy::Path | QueryStrategy::Metadata => coverage,
    };
    (score_gate * 0.2 + strategy_gate * 0.8).clamp(0.0, 1.0)
}

fn rewrite_plan(plan: &QueryPlan) -> QueryPlan {
    let mut rewritten = plan.clone();
    rewritten.strategy = match plan.strategy {
        QueryStrategy::ExactSymbol => QueryStrategy::Lexical,
        QueryStrategy::Lexical => {
            if plan.filters.path.is_some() || plan.filters.language.is_some() {
                QueryStrategy::Metadata
            } else {
                QueryStrategy::Path
            }
        }
        QueryStrategy::Path | QueryStrategy::Metadata => QueryStrategy::Lexical,
    };
    rewritten.reason = "low_coverage_rewrite".into();
    rewritten.confidence = (rewritten.confidence - 0.2).max(0.0);
    rewritten
}

fn hybrid_allowed(plan: &QueryPlan, config: &HybridConfig) -> bool {
    let language = plan.filters.language.as_deref();
    let path = plan.filters.path.as_deref();
    (config.allowed_languages.is_empty()
        || language.is_some_and(|value| {
            config
                .allowed_languages
                .iter()
                .any(|allowed| allowed == value)
        }))
        && (config.allowed_path_prefixes.is_empty()
            || path.is_some_and(|value| {
                config
                    .allowed_path_prefixes
                    .iter()
                    .any(|allowed| value.starts_with(allowed))
            }))
}

pub fn build_vector_index(
    connection: &mut Connection,
    workspace_root: &Path,
    config: &HybridConfig,
    cancelled: impl Fn() -> bool,
) -> Result<Option<String>, RagError> {
    if !config.enabled {
        return Ok(None);
    }
    if !(1_000..=10 * 60 * 1000).contains(&config.build_timeout_ms)
        || !(1024 * 1024..=2 * 1024 * 1024 * 1024).contains(&config.max_build_bytes)
    {
        return Err(RagError::InvalidConfig(
            "hybrid resource limits are invalid".into(),
        ));
    }
    let started = Instant::now();
    let key = workspace_key(workspace_root)?;
    let generation = active_generation(connection, &key)?.ok_or(RagError::IndexUnavailable)?;
    let index_id = format!(
        "vector-{}-{}",
        generation,
        &sha256_hex(format!("{key}:{generation}:{LOCAL_EMBEDDING_VERSION}").as_bytes())[..16]
    );
    connection.execute(
        "INSERT OR REPLACE INTO workspace_vector_indexes
         (index_id, workspace_key, source_generation, embedding_model_id,
          embedding_model_version, vector_dimension, distance_metric, normalization,
          chunker_version, build_status, created_at, vector_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'cosine', 'l2', ?7, 'building', ?8, 0)",
        params![
            index_id,
            key,
            generation,
            LOCAL_EMBEDDING_MODEL,
            LOCAL_EMBEDDING_VERSION,
            VECTOR_DIMENSION as i64,
            CHUNKER_VERSION,
            now_ms()
        ],
    )?;
    connection.execute(
        "DELETE FROM workspace_chunk_vectors WHERE index_id = ?1",
        [&index_id],
    )?;
    let rows = {
        let mut statement = connection.prepare(
            "SELECT c.chunk_id, c.chunk_text, d.language, d.path
             FROM document_chunks c JOIN workspace_documents d ON d.document_id = c.document_id
             WHERE c.workspace_key = ?1 AND c.generation = ?2 AND d.status = 'active'
               AND d.redaction_status != 'full' AND d.is_secret_path = 0
             ORDER BY d.path, c.ordinal",
        )?;
        let collected = statement
            .query_map(params![key, generation], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        collected
    };
    let mut bytes = 0u64;
    for (chunk_id, text, language, path) in &rows {
        if cancelled() {
            fail_vector_build(connection, &index_id, "cancelled")?;
            return Err(RagError::Cancelled);
        }
        if started.elapsed() > Duration::from_millis(config.build_timeout_ms) {
            fail_vector_build(connection, &index_id, "failed")?;
            return Err(RagError::Timeout);
        }
        if !config.allowed_languages.is_empty() && !config.allowed_languages.contains(language) {
            continue;
        }
        if !config.allowed_path_prefixes.is_empty()
            && !config
                .allowed_path_prefixes
                .iter()
                .any(|prefix| path.starts_with(prefix))
        {
            continue;
        }
        let vector = embed_local(text);
        let blob = encode_vector(&vector);
        bytes += blob.len() as u64;
        if bytes > config.max_build_bytes {
            fail_vector_build(connection, &index_id, "failed")?;
            return Err(RagError::InvalidConfig(
                "vector build memory budget exceeded".into(),
            ));
        }
        connection.execute(
            "INSERT INTO workspace_chunk_vectors(index_id, chunk_id, vector) VALUES (?1, ?2, ?3)",
            params![index_id, chunk_id, blob],
        )?;
    }
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM workspace_chunk_vectors WHERE index_id = ?1",
        [&index_id],
        |row| row.get(0),
    )?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "UPDATE workspace_vector_indexes SET build_status = 'ready', vector_count = ?2
         WHERE index_id = ?1 AND build_status = 'building'",
        params![index_id, count],
    )?;
    transaction.execute(
        "UPDATE workspace_vector_indexes SET build_status = 'deprecated'
         WHERE workspace_key = ?1 AND build_status = 'published'",
        [&key],
    )?;
    transaction.execute(
        "UPDATE workspace_vector_indexes SET build_status = 'published', published_at = ?2
         WHERE index_id = ?1 AND build_status = 'ready'",
        params![index_id, now_ms()],
    )?;
    transaction.commit()?;
    Ok(Some(index_id))
}

fn fail_vector_build(
    connection: &Connection,
    index_id: &str,
    status: &str,
) -> Result<(), RagError> {
    connection.execute(
        "DELETE FROM workspace_chunk_vectors WHERE index_id = ?1",
        [index_id],
    )?;
    connection.execute(
        "UPDATE workspace_vector_indexes SET build_status = ?2 WHERE index_id = ?1",
        params![index_id, status],
    )?;
    Ok(())
}

fn embed_local(text: &str) -> Vec<f32> {
    let mut vector = vec![0.0f32; VECTOR_DIMENSION];
    for term in bounded_terms(text) {
        let hash = digest(&SHA256, term.to_lowercase().as_bytes());
        let bytes = hash.as_ref();
        let index = u16::from_le_bytes([bytes[0], bytes[1]]) as usize % VECTOR_DIMENSION;
        let sign = if bytes[2] & 1 == 0 { 1.0 } else { -1.0 };
        vector[index] += sign;
    }
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        vector.iter_mut().for_each(|value| *value /= norm);
    }
    vector
}

fn encode_vector(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn decode_vector(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() != VECTOR_DIMENSION * 4 {
        return None;
    }
    Some(
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| f32::from_le_bytes(*chunk))
            .collect(),
    )
}

struct HybridRetrievalInput<'a> {
    connection: &'a Connection,
    workspace_root: &'a Path,
    workspace_key: &'a str,
    generation: i64,
    query: &'a str,
    filters: &'a QueryFilters,
    lexical: &'a [RetrievedChunk],
    limits: &'a RetrievalLimits,
}

fn hybrid_retrieval(
    input: HybridRetrievalInput<'_>,
) -> Result<Option<Vec<RetrievedChunk>>, RagError> {
    let HybridRetrievalInput {
        connection,
        workspace_root,
        workspace_key,
        generation,
        query,
        filters,
        lexical,
        limits,
    } = input;
    let metadata = connection
        .query_row(
            "SELECT index_id, source_generation, embedding_model_id, embedding_model_version,
                    vector_dimension, distance_metric, normalization, chunker_version
             FROM workspace_vector_indexes
             WHERE workspace_key = ?1 AND build_status = 'published'",
            [workspace_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((
        index_id,
        source_generation,
        model,
        version,
        dimension,
        metric,
        normalization,
        chunker,
    )) = metadata
    else {
        return Ok(None);
    };
    if source_generation != generation
        || model != LOCAL_EMBEDDING_MODEL
        || version != LOCAL_EMBEDDING_VERSION
        || dimension != VECTOR_DIMENSION as i64
        || metric != "cosine"
        || normalization != "l2"
        || chunker != CHUNKER_VERSION
    {
        return Err(RagError::InvalidConfig("vector_index_incompatible".into()));
    }
    let query_vector = embed_local(query);
    let mut statement = connection.prepare(
        "SELECT v.chunk_id, v.vector FROM workspace_chunk_vectors v
         JOIN document_chunks c ON c.chunk_id = v.chunk_id
         JOIN workspace_documents d ON d.document_id = c.document_id
         WHERE v.index_id = ?1 AND c.workspace_key = ?2 AND c.generation = ?3
           AND d.status = 'active' AND d.redaction_status != 'full' AND d.is_secret_path = 0
           AND (?4 = '' OR d.path = ?4 OR d.path LIKE ?4 || '/%')
           AND (?5 = '' OR d.language = ?5)",
    )?;
    let path_filter = filters.path.as_deref().unwrap_or("");
    let language_filter = filters.language.as_deref().unwrap_or("");
    let mut vector_scores = statement
        .query_map(
            params![
                index_id,
                workspace_key,
                generation,
                path_filter,
                language_filter
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )?
        .filter_map(Result::ok)
        .filter_map(|(id, blob)| {
            decode_vector(&blob).map(|vector| (id, dot(&query_vector, &vector)))
        })
        .collect::<Vec<_>>();
    vector_scores.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    vector_scores.truncate(limits.max_retrieval_chunks);
    let vector_ranks = vector_scores
        .iter()
        .enumerate()
        .map(|(index, (id, _))| (id.clone(), index + 1))
        .collect::<HashMap<_, _>>();
    let lexical_ranks = lexical
        .iter()
        .enumerate()
        .map(|(index, item)| (item.chunk_id.clone(), index + 1))
        .collect::<HashMap<_, _>>();
    let mut fused = lexical.to_vec();
    let lexical_ids = lexical
        .iter()
        .map(|item| item.chunk_id.as_str())
        .collect::<HashSet<_>>();
    for (chunk_id, _) in &vector_scores {
        if !lexical_ids.contains(chunk_id.as_str()) {
            if let Some(candidate) = load_vector_candidate(
                connection,
                workspace_root,
                workspace_key,
                generation,
                chunk_id,
                filters,
            )? {
                fused.push(candidate);
            }
        }
    }
    for item in &mut fused {
        let lexical_rank = lexical_ranks.get(&item.chunk_id).copied();
        let vector_rank = vector_ranks.get(&item.chunk_id).copied();
        let rrf = lexical_rank
            .map(|rank| 1.0 / (RRF_K + rank as f64))
            .unwrap_or(0.0)
            + vector_rank
                .map(|rank| 1.0 / (RRF_K + rank as f64))
                .unwrap_or(0.0);
        item.score = rrf;
        item.ranking_explanation = RankingExplanation {
            algorithm: "rrf".into(),
            lexical_rank,
            vector_rank,
            rrf_rank: 0,
            sources: [
                lexical_rank.map(|_| "lexical".to_string()),
                vector_rank.map(|_| "vector".to_string()),
            ]
            .into_iter()
            .flatten()
            .collect(),
        };
    }
    fused.sort_by(deterministic_rank);
    for (index, item) in fused.iter_mut().enumerate() {
        item.ranking_explanation.rrf_rank = index + 1;
    }
    fused.truncate(limits.max_retrieval_chunks);
    Ok(Some(fused))
}

fn load_vector_candidate(
    connection: &Connection,
    workspace_root: &Path,
    workspace_key: &str,
    generation: i64,
    chunk_id: &str,
    filters: &QueryFilters,
) -> Result<Option<RetrievedChunk>, RagError> {
    let row = connection
        .query_row(
            "SELECT c.document_id, d.path, d.language, c.byte_start, c.byte_end,
                    c.chunk_hash, d.file_hash, c.chunk_text, c.symbol,
                    c.parent_context, d.redaction_status, d.size_bytes
             FROM document_chunks c
             JOIN workspace_documents d ON d.document_id = c.document_id
             WHERE c.workspace_key = ?1 AND c.generation = ?2 AND c.chunk_id = ?3
               AND d.status = 'active' AND d.redaction_status != 'full'
               AND d.is_secret_path = 0
               AND (?4 = '' OR d.path = ?4 OR d.path LIKE ?4 || '/%')
               AND (?5 = '' OR d.language = ?5)",
            params![
                workspace_key,
                generation,
                chunk_id,
                filters.path.as_deref().unwrap_or(""),
                filters.language.as_deref().unwrap_or("")
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, i64>(11)?,
                ))
            },
        )
        .optional()?;
    let Some((
        source_id,
        path,
        language,
        byte_start,
        byte_end,
        chunk_hash,
        file_hash,
        indexed_content,
        symbol,
        parent_context,
        redaction_status,
        indexed_size,
    )) = row
    else {
        return Ok(None);
    };
    let root = workspace_root.canonicalize()?;
    let validation = validate_source(
        &root,
        &root.join(&path),
        &file_hash,
        indexed_size as u64,
        byte_start as usize,
        byte_end as usize,
    );
    let (content, lines, stale) = match validation {
        Ok(bytes) => match decode_source_range(&bytes, byte_start as usize, byte_end as usize) {
            Ok((content, lines)) if content.trim() == indexed_content.trim() => {
                (Some(content.trim().to_string()), Some(lines), false)
            }
            _ => (None, None, true),
        },
        Err(_) => (None, None, true),
    };
    Ok(Some(RetrievedChunk {
        source_id,
        chunk_id: chunk_id.to_string(),
        relative_path: path,
        language,
        byte_start: byte_start as u64,
        byte_end: byte_end as u64,
        lines,
        chunk_hash,
        content_hash: file_hash,
        content,
        symbol,
        parent_context,
        score: 0.0,
        score_explanation: ScoreExplanation {
            algorithm: "vector".into(),
            column_weights: BTreeMap::new(),
            term_frequencies: BTreeMap::new(),
            document_length: indexed_content.len(),
            matched_filters: vec![format!("workspace_key={workspace_key}")],
            excluded_by: Vec::new(),
        },
        ranking_explanation: RankingExplanation {
            algorithm: "vector".into(),
            lexical_rank: None,
            vector_rank: None,
            rrf_rank: 0,
            sources: vec!["vector".into()],
        },
        stale,
        redaction_status,
        checker_confidence: 0.0,
    }))
}

fn dot(left: &[f32], right: &[f32]) -> f64 {
    left.iter()
        .zip(right)
        .map(|(a, b)| (*a as f64) * (*b as f64))
        .sum()
}
