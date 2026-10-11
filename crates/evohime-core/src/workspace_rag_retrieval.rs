use super::*;

pub fn stable_id(workspace_key: &str, generation: i64, value: &str, kind: &str) -> String {
    format!(
        "{kind}-{}",
        &sha256_hex(format!("{workspace_key}:{generation}:{kind}:{value}").as_bytes())[..32]
    )
}

pub fn bounded_error(path: &str, error: &str) -> String {
    let lower = error.to_lowercase();
    let code = if lower.contains("permission") {
        "permission_denied"
    } else if lower.contains("not found") {
        "file_not_found"
    } else if lower.contains("timeout") {
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

struct LexicalCandidate {
    chunk_id: String,
    document_id: String,
    path: String,
    language: String,
    byte_start: i64,
    byte_end: i64,
    chunk_hash: String,
    file_hash: String,
    indexed_content: String,
    symbol: Option<String>,
    parent_context: String,
    redaction: String,
    indexed_size: i64,
    bm25: f64,
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
    let mut match_expression = String::new();
    for (index, term) in terms.iter().enumerate() {
        if index > 0 {
            match_expression.push_str(" OR ");
        }
        match_expression.push('"');
        for character in term.chars() {
            if character == '"' {
                match_expression.push_str("\"\"");
            } else {
                match_expression.push(character);
            }
        }
        match_expression.push('"');
    }
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
                Ok(LexicalCandidate {
                    chunk_id: row.get(0)?,
                    document_id: row.get(1)?,
                    path: row.get(2)?,
                    language: row.get(3)?,
                    byte_start: row.get(4)?,
                    byte_end: row.get(5)?,
                    chunk_hash: row.get(6)?,
                    file_hash: row.get(7)?,
                    indexed_content: row.get(8)?,
                    symbol: row.get(9)?,
                    parent_context: row.get(10)?,
                    redaction: row.get(11)?,
                    indexed_size: row.get(12)?,
                    bm25: row.get(13)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let canonical_root = workspace_root.canonicalize()?;
    let mut candidates_by_path = BTreeMap::<String, Vec<(usize, LexicalCandidate)>>::new();
    for (rank, candidate) in candidates.into_iter().enumerate() {
        candidates_by_path
            .entry(candidate.path.clone())
            .or_default()
            .push((rank, candidate));
    }
    let mut output = Vec::new();
    for (path, candidates) in candidates_by_path {
        let Some((_, first)) = candidates.first() else {
            continue;
        };
        let validation = validate_source(
            &canonical_root,
            &canonical_root.join(&path),
            &first.file_hash,
            first.indexed_size as u64,
            0,
            first.indexed_size as usize,
        );
        let line_index = validation
            .as_ref()
            .ok()
            .map(|bytes| source_line_breaks(bytes))
            .unwrap_or_default();
        for (rank, candidate) in candidates {
            let mut matched_filters = vec![format!("workspace_key={workspace_key}")];
            if !path_filter.is_empty() {
                matched_filters.push(format!("path={path_filter}"));
            }
            if !language_filter.is_empty() {
                matched_filters.push(format!("language={language_filter}"));
            }
            let score = -candidate.bm25;
            let (content, lines, stale) = match validation.as_ref() {
                Ok(bytes) if candidate.redaction != "full" => {
                    match decode_source_range_with_line_breaks(
                        bytes,
                        candidate.byte_start as usize,
                        candidate.byte_end as usize,
                        &line_index,
                    ) {
                        Ok((content, lines))
                            if content.trim() == candidate.indexed_content.trim() =>
                        {
                            (Some(content.trim().to_string()), Some(lines), false)
                        }
                        _ => (None, None, true),
                    }
                }
                Ok(_) => (None, None, false),
                Err(_) => (None, None, true),
            };
            let indexed_lower = candidate.indexed_content.to_lowercase();
            let term_frequencies = terms
                .iter()
                .zip(&normalized_terms)
                .map(|(term, normalized_term)| {
                    let count = indexed_lower.matches(normalized_term).count();
                    (term.clone(), count)
                })
                .collect::<BTreeMap<_, _>>();
            output.push(RetrievedChunk {
                source_id: candidate.document_id,
                chunk_id: candidate.chunk_id,
                relative_path: candidate.path,
                language: candidate.language,
                byte_start: candidate.byte_start as u64,
                byte_end: candidate.byte_end as u64,
                lines,
                chunk_hash: candidate.chunk_hash,
                content_hash: candidate.file_hash,
                content,
                symbol: candidate.symbol,
                parent_context: candidate.parent_context,
                score,
                score_explanation: ScoreExplanation {
                    algorithm: "bm25".into(),
                    column_weights: BTreeMap::from([
                        ("content".into(), 1.0),
                        ("symbol_normalized".into(), 2.0),
                        ("canonical_path".into(), 0.5),
                    ]),
                    term_frequencies,
                    document_length: candidate.indexed_content.len(),
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
                redaction_status: candidate.redaction,
                checker_confidence: 0.0,
            });
        }
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
    const MAX_REVALIDATION_SOURCE_BYTES: u64 = 16 * 1024 * 1024;
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(root) {
        return Err(RagError::Sandbox("retrieval path escaped workspace".into()));
    }
    if expected_size > MAX_REVALIDATION_SOURCE_BYTES {
        return Err(RagError::InvalidWorkspace(
            "source exceeds reread limit".into(),
        ));
    }
    let bytes = read_bounded_source(&canonical)?;
    if bytes.len() as u64 != expected_size
        || sha256_hex(&bytes) != expected_hash
        || byte_start > byte_end
        || byte_end > bytes.len()
    {
        return Err(RagError::InvalidWorkspace("stale source snapshot".into()));
    }
    Ok(bytes)
}

pub fn read_bounded_source(path: &Path) -> Result<Vec<u8>, RagError> {
    const MAX_REVALIDATION_SOURCE_BYTES: u64 = 16 * 1024 * 1024;
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_REVALIDATION_SOURCE_BYTES {
        return Err(RagError::InvalidWorkspace(
            "source exceeds reread limit".into(),
        ));
    }
    Ok(fs::read(path)?)
}

#[cfg(test)]
fn decode_source_range(
    bytes: &[u8],
    byte_start: usize,
    byte_end: usize,
) -> Result<(String, [u64; 2]), RagError> {
    let line_breaks = source_line_breaks(bytes);
    decode_source_range_with_line_breaks(bytes, byte_start, byte_end, &line_breaks)
}

#[derive(Default)]
struct SourceLineIndex {
    utf16: bool,
    line_counts: Vec<usize>,
    invalid_counts: Vec<usize>,
}

fn source_line_breaks(bytes: &[u8]) -> SourceLineIndex {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little_endian = bytes.starts_with(&[0xff, 0xfe]);
        let unit_count = (bytes.len() - 2) / 2;
        let mut line_counts = vec![0];
        let mut invalid_counts = vec![0];
        let mut line_count = 0usize;
        let mut invalid_count = 0usize;
        let mut index = 0;
        let mut next_checkpoint = 2 + 256;
        while index < unit_count {
            let byte_start = 2 + index * 2;
            while byte_start >= next_checkpoint {
                line_counts.push(line_count);
                invalid_counts.push(invalid_count);
                next_checkpoint += 256;
            }
            let unit = utf16_unit(bytes, byte_start, little_endian);
            if (0xd800..=0xdbff).contains(&unit) {
                if index + 1 < unit_count
                    && (0xdc00..=0xdfff).contains(&utf16_unit(bytes, byte_start + 2, little_endian))
                {
                    index += 2;
                    continue;
                }
                invalid_count += 1;
            } else if (0xdc00..=0xdfff).contains(&unit) {
                invalid_count += 1;
            }
            if unit == b'\n' as u16 {
                line_count += 1;
            }
            index += 1;
        }
        while next_checkpoint <= bytes.len() {
            line_counts.push(line_count);
            invalid_counts.push(invalid_count);
            next_checkpoint += 256;
        }
        return SourceLineIndex {
            utf16: true,
            line_counts,
            invalid_counts,
        };
    }
    let mut line_counts = vec![0];
    let mut line_count = 0usize;
    for block in bytes.chunks(256) {
        line_count += block.iter().filter(|byte| **byte == b'\n').count();
        line_counts.push(line_count);
    }
    SourceLineIndex {
        utf16: false,
        line_counts,
        invalid_counts: Vec::new(),
    }
}

fn utf16_unit(bytes: &[u8], offset: usize, little_endian: bool) -> u16 {
    if little_endian {
        u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
    } else {
        u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
    }
}

fn indexed_count_to(bytes: &[u8], line_index: &SourceLineIndex, offset: usize) -> usize {
    if line_index.utf16 {
        let offset = offset.max(2);
        let block = (offset - 2) / 256;
        let block_start = 2 + block * 256;
        let mut count = line_index
            .line_counts
            .get(block)
            .copied()
            .unwrap_or_default();
        for pair in bytes[block_start..offset].as_chunks::<2>().0 {
            if *pair == [b'\n', 0] || *pair == [0, b'\n'] {
                count += 1;
            }
        }
        count
    } else {
        let block = offset / 256;
        let block_start = block * 256;
        line_index
            .line_counts
            .get(block)
            .copied()
            .unwrap_or_default()
            + bytes[block_start..offset]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
    }
}

fn indexed_invalid_utf16_count_to(
    bytes: &[u8],
    line_index: &SourceLineIndex,
    offset: usize,
    little_endian: bool,
) -> usize {
    let offset = offset.max(2);
    let block = (offset - 2) / 256;
    let block_start = 2 + block * 256;
    let mut invalid = line_index
        .invalid_counts
        .get(block)
        .copied()
        .unwrap_or_default();
    let mut byte_start = block_start;
    if byte_start >= 4
        && byte_start + 1 < offset
        && (0xd800..=0xdbff).contains(&utf16_unit(bytes, byte_start - 2, little_endian))
        && (0xdc00..=0xdfff).contains(&utf16_unit(bytes, byte_start, little_endian))
    {
        byte_start += 2;
    }
    while byte_start + 1 < offset {
        let unit = utf16_unit(bytes, byte_start, little_endian);
        if (0xd800..=0xdbff).contains(&unit) {
            if byte_start + 3 < offset
                && (0xdc00..=0xdfff).contains(&utf16_unit(bytes, byte_start + 2, little_endian))
            {
                byte_start += 4;
                continue;
            }
            invalid += 1;
        } else if (0xdc00..=0xdfff).contains(&unit) {
            invalid += 1;
        }
        byte_start += 2;
    }
    invalid
}

fn utf16_boundary_splits_pair(bytes: &[u8], boundary: usize, little_endian: bool) -> bool {
    boundary >= 4
        && boundary + 1 < bytes.len()
        && (0xd800..=0xdbff).contains(&utf16_unit(bytes, boundary - 2, little_endian))
        && (0xdc00..=0xdfff).contains(&utf16_unit(bytes, boundary, little_endian))
}

fn decode_source_range_with_line_breaks(
    bytes: &[u8],
    byte_start: usize,
    byte_end: usize,
    line_index: &SourceLineIndex,
) -> Result<(String, [u64; 2]), RagError> {
    if byte_start > byte_end || byte_end > bytes.len() {
        return Err(RagError::InvalidWorkspace(
            "invalid source byte range".into(),
        ));
    }
    let (content, preceding_newlines) =
        if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
            let little_endian = bytes.starts_with(&[0xff, 0xfe]);
            let decode = |slice: &[u8]| -> Result<String, RagError> {
                if !slice.len().is_multiple_of(2) {
                    return Err(RagError::InvalidWorkspace(
                        "unaligned UTF-16 byte range".into(),
                    ));
                }
                let units = slice.as_chunks::<2>().0.iter().map(|pair| {
                    if little_endian {
                        u16::from_le_bytes(*pair)
                    } else {
                        u16::from_be_bytes(*pair)
                    }
                });
                char::decode_utf16(units)
                    .collect::<Result<String, _>>()
                    .map_err(|_| RagError::InvalidWorkspace("invalid UTF-16 source range".into()))
            };
            let start = byte_start.max(2);
            let end = byte_end.max(2);
            let invalid_before_start =
                indexed_invalid_utf16_count_to(bytes, line_index, start, little_endian) > 0;
            let invalid_in_range =
                indexed_invalid_utf16_count_to(bytes, line_index, end, little_endian)
                    > indexed_invalid_utf16_count_to(bytes, line_index, start, little_endian);
            if !(start - 2).is_multiple_of(2)
                || invalid_before_start
                || invalid_in_range
                || utf16_boundary_splits_pair(bytes, start, little_endian)
                || utf16_boundary_splits_pair(bytes, end, little_endian)
            {
                return Err(RagError::InvalidWorkspace(
                    "invalid UTF-16 source range".into(),
                ));
            }
            (
                decode(&bytes[start..end])?,
                indexed_count_to(bytes, line_index, byte_start) as u64,
            )
        } else {
            (
                String::from_utf8_lossy(&bytes[byte_start..byte_end]).into_owned(),
                indexed_count_to(bytes, line_index, byte_start) as u64,
            )
        };
    let start_line = preceding_newlines + 1;
    let newlines_in_range = indexed_count_to(bytes, line_index, byte_end)
        .saturating_sub(indexed_count_to(bytes, line_index, byte_start));
    let end_line = start_line + newlines_in_range as u64;
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
    const VECTOR_BUILD_BATCH_ROWS: usize = 128;
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
    let mut last_path: Option<String> = None;
    let mut last_ordinal = -1i64;
    let mut bytes = 0u64;
    loop {
        if cancelled() {
            fail_vector_build(connection, &index_id, "cancelled")?;
            return Err(RagError::Cancelled);
        }
        if started.elapsed() > Duration::from_millis(config.build_timeout_ms) {
            fail_vector_build(connection, &index_id, "failed")?;
            return Err(RagError::Timeout);
        }
        let rows = {
            let mut statement = connection.prepare(
                "SELECT c.chunk_id, c.chunk_text, d.language, d.path, c.ordinal
                 FROM document_chunks c
                 JOIN workspace_documents d ON d.document_id = c.document_id
                 WHERE c.workspace_key = ?1 AND c.generation = ?2 AND d.status = 'active'
                   AND d.redaction_status != 'full' AND d.is_secret_path = 0
                   AND (?3 IS NULL OR d.path > ?3 OR (d.path = ?3 AND c.ordinal > ?4))
                 ORDER BY d.path, c.ordinal
                 LIMIT ?5",
            )?;
            let rows = statement
                .query_map(
                    params![
                        key,
                        generation,
                        last_path,
                        last_ordinal,
                        VECTOR_BUILD_BATCH_ROWS as i64
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                        ))
                    },
                )?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        let Some((_, _, _, final_path, final_ordinal)) = rows.last() else {
            break;
        };
        last_path = Some(final_path.clone());
        last_ordinal = *final_ordinal;

        let mut vectors = Vec::with_capacity(rows.len());
        for (chunk_id, text, language, path, _) in &rows {
            if cancelled() {
                fail_vector_build(connection, &index_id, "cancelled")?;
                return Err(RagError::Cancelled);
            }
            if started.elapsed() > Duration::from_millis(config.build_timeout_ms) {
                fail_vector_build(connection, &index_id, "failed")?;
                return Err(RagError::Timeout);
            }
            if !config.allowed_languages.is_empty() && !config.allowed_languages.contains(language)
            {
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
            let blob = encode_vector(&embed_local(text));
            bytes += blob.len() as u64;
            if bytes > config.max_build_bytes {
                fail_vector_build(connection, &index_id, "failed")?;
                return Err(RagError::InvalidConfig(
                    "vector build memory budget exceeded".into(),
                ));
            }
            vectors.push((chunk_id, blob));
        }
        if !vectors.is_empty() {
            let transaction = connection.transaction()?;
            {
                let mut insert = transaction.prepare_cached(
                    "INSERT INTO workspace_chunk_vectors(index_id, chunk_id, vector)
                     VALUES (?1, ?2, ?3)",
                )?;
                for (chunk_id, blob) in vectors {
                    insert.execute(params![index_id, chunk_id, blob])?;
                }
            }
            transaction.commit()?;
        }
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

#[cfg(test)]
fn decode_vector(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() != VECTOR_DIMENSION * 4 {
        return None;
    }
    let vector = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect::<Vec<_>>();
    vector
        .iter()
        .all(|value| value.is_finite())
        .then_some(vector)
}

fn dot_encoded(left: &[f32], bytes: &[u8]) -> Option<f64> {
    if bytes.len() != left.len() * 4 || !left.iter().all(|value| value.is_finite()) {
        return None;
    }
    let mut score = 0.0;
    for (left, right) in left.iter().zip(bytes.as_chunks::<4>().0) {
        let right = f32::from_le_bytes(*right);
        if !right.is_finite() {
            return None;
        }
        score += f64::from(*left) * f64::from(right);
    }
    Some(score)
}

#[derive(Debug)]
struct ScoredVector {
    chunk_id: String,
    score: f64,
}

impl PartialEq for ScoredVector {
    fn eq(&self, other: &Self) -> bool {
        self.chunk_id == other.chunk_id && self.score.total_cmp(&other.score) == Ordering::Equal
    }
}

impl Eq for ScoredVector {}

impl PartialOrd for ScoredVector {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScoredVector {
    fn cmp(&self, other: &Self) -> Ordering {
        vector_score_order(self, other)
    }
}

fn vector_score_order(left: &ScoredVector, right: &ScoredVector) -> Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.chunk_id.cmp(&right.chunk_id))
}

fn push_top_vector_score(
    heap: &mut std::collections::BinaryHeap<ScoredVector>,
    chunk_id: &str,
    score: f64,
    limit: usize,
) {
    if limit == 0 {
        return;
    }
    if heap.len() < limit {
        heap.push(ScoredVector {
            chunk_id: chunk_id.to_owned(),
            score,
        });
        return;
    }
    let belongs_in_top = heap.peek().is_some_and(|worst| {
        score.total_cmp(&worst.score) == Ordering::Greater
            || (score.total_cmp(&worst.score) == Ordering::Equal
                && chunk_id.cmp(worst.chunk_id.as_str()) == Ordering::Less)
    });
    if belongs_in_top {
        let _ = heap.pop();
        heap.push(ScoredVector {
            chunk_id: chunk_id.to_owned(),
            score,
        });
    }
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
    let path_filter = filters.path.as_deref().unwrap_or("");
    let language_filter = filters.language.as_deref().unwrap_or("");
    let vector_scores = {
        let mut statement = connection.prepare(
            "SELECT v.chunk_id, v.vector FROM workspace_chunk_vectors v
             JOIN document_chunks c ON c.chunk_id = v.chunk_id
             JOIN workspace_documents d ON d.document_id = c.document_id
             WHERE v.index_id = ?1 AND c.workspace_key = ?2 AND c.generation = ?3
               AND d.status = 'active' AND d.redaction_status != 'full' AND d.is_secret_path = 0
               AND (?4 = '' OR d.path = ?4 OR d.path LIKE ?4 || '/%')
               AND (?5 = '' OR d.language = ?5)",
        )?;
        let mut rows = statement.query(params![
            index_id,
            workspace_key,
            generation,
            path_filter,
            language_filter
        ])?;
        let mut top = std::collections::BinaryHeap::with_capacity(limits.max_retrieval_chunks);
        while let Some(row) = rows.next()? {
            let chunk_id = row.get_ref(0)?.as_str().map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            let blob = row.get_ref(1)?.as_blob().map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Blob,
                    Box::new(error),
                )
            })?;
            if let Some(score) = dot_encoded(&query_vector, blob) {
                push_top_vector_score(&mut top, chunk_id, score, limits.max_retrieval_chunks);
            }
        }
        let mut scores = top.into_vec();
        scores.sort_by(vector_score_order);
        scores
            .into_iter()
            .map(|scored| (scored.chunk_id, scored.score))
            .collect::<Vec<_>>()
    };
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
    let vector_candidate_ids = vector_scores
        .iter()
        .filter_map(|(chunk_id, _)| {
            (!lexical_ids.contains(chunk_id.as_str())).then_some(chunk_id.as_str())
        })
        .collect::<Vec<_>>();
    fused.extend(load_vector_candidates(
        connection,
        workspace_root,
        workspace_key,
        generation,
        &vector_candidate_ids,
        filters,
    )?);
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

fn load_vector_candidates(
    connection: &Connection,
    workspace_root: &Path,
    workspace_key: &str,
    generation: i64,
    chunk_ids: &[&str],
    filters: &QueryFilters,
) -> Result<Vec<RetrievedChunk>, RagError> {
    if chunk_ids.is_empty() {
        return Ok(Vec::new());
    }

    // The retrieval limit is at most 200, so this bounded VALUES query stays
    // well below SQLite's parameter limit while replacing one query per hit.
    let requested = chunk_ids
        .iter()
        .enumerate()
        .map(|(index, _)| format!("(?{}, {})", index + 1, index))
        .collect::<Vec<_>>()
        .join(", ");
    let workspace_parameter = chunk_ids.len() + 1;
    let generation_parameter = chunk_ids.len() + 2;
    let path_parameter = chunk_ids.len() + 3;
    let language_parameter = chunk_ids.len() + 4;
    let sql = format!(
        "WITH requested(chunk_id, vector_rank) AS (VALUES {requested})
         SELECT c.chunk_id, c.document_id, d.path, d.language, c.byte_start,
                c.byte_end, c.chunk_hash, d.file_hash, c.chunk_text, c.symbol,
                c.parent_context, d.redaction_status, d.size_bytes
         FROM requested r
         JOIN document_chunks c ON c.chunk_id = r.chunk_id
         JOIN workspace_documents d ON d.document_id = c.document_id
         WHERE c.workspace_key = ?{workspace_parameter}
           AND c.generation = ?{generation_parameter}
           AND d.status = 'active' AND d.redaction_status != 'full'
           AND d.is_secret_path = 0
           AND (?{path_parameter} = '' OR d.path = ?{path_parameter} OR d.path LIKE ?{path_parameter} || '/%')
           AND (?{language_parameter} = '' OR d.language = ?{language_parameter})
         ORDER BY r.vector_rank",
    );
    let mut statement = connection.prepare(&sql)?;
    let mut query_parameters = chunk_ids
        .iter()
        .map(|chunk_id| rusqlite::types::Value::Text((*chunk_id).to_string()))
        .collect::<Vec<_>>();
    query_parameters.extend([
        rusqlite::types::Value::Text(workspace_key.to_string()),
        rusqlite::types::Value::Integer(generation),
        rusqlite::types::Value::Text(filters.path.as_deref().unwrap_or("").to_string()),
        rusqlite::types::Value::Text(filters.language.as_deref().unwrap_or("").to_string()),
    ]);
    let rows = statement
        .query_map(params_from_iter(query_parameters.iter()), |row| {
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
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    let canonical_root = workspace_root.canonicalize()?;
    let mut source_cache =
        HashMap::<(String, String, i64), Option<(Vec<u8>, SourceLineIndex)>>::new();
    let mut candidates = Vec::with_capacity(chunk_ids.len());
    for row in rows {
        let (
            chunk_id,
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
        ) = row;
        let cache_key = (path.clone(), file_hash.clone(), indexed_size);
        let source = source_cache.entry(cache_key).or_insert_with(|| {
            let expected_size = u64::try_from(indexed_size).ok()?;
            let byte_end = usize::try_from(expected_size).ok()?;
            let bytes = validate_source(
                &canonical_root,
                &canonical_root.join(&path),
                &file_hash,
                expected_size,
                0,
                byte_end,
            )
            .ok()?;
            let line_index = source_line_breaks(&bytes);
            Some((bytes, line_index))
        });
        let (content, lines, stale) = match source {
            Some((bytes, line_index)) => match decode_source_range_with_line_breaks(
                bytes,
                byte_start as usize,
                byte_end as usize,
                line_index,
            ) {
                Ok((content, lines)) if content.trim() == indexed_content.trim() => {
                    (Some(content.trim().to_string()), Some(lines), false)
                }
                _ => (None, None, true),
            },
            None => (None, None, true),
        };
        candidates.push(RetrievedChunk {
            source_id,
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
        });
    }
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_line_ranges_count_prefix_without_decoding_utf8_text() {
        let bytes = "a\nb\nc\n".as_bytes();
        let line_index = source_line_breaks(bytes);
        assert_eq!(
            decode_source_range_with_line_breaks(bytes, 4, 6, &line_index).unwrap(),
            ("c\n".into(), [3, 4])
        );
        assert_eq!(
            decode_source_range_with_line_breaks(bytes, 0, 2, &line_index).unwrap(),
            ("a\n".into(), [1, 2])
        );

        let dense = vec![b'\n'; 16 * 1024];
        let dense_index = source_line_breaks(&dense);
        assert!(dense_index.line_counts.len() <= dense.len() / 256 + 1);
        assert_eq!(
            indexed_count_to(&dense, &dense_index, dense.len()),
            dense.len()
        );
    }

    #[test]
    fn source_line_ranges_count_utf16_prefix_and_reject_invalid_surrogates() {
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend("a\nb\nc\n".encode_utf16().flat_map(u16::to_le_bytes));
        let line_index = source_line_breaks(&bytes);
        assert_eq!(
            decode_source_range_with_line_breaks(&bytes, 10, 14, &line_index).unwrap(),
            ("c\n".into(), [3, 4])
        );

        let mut checkpointed = vec![0xff, 0xfe];
        checkpointed.extend("a\n".repeat(96).encode_utf16().flat_map(u16::to_le_bytes));
        let line_index = source_line_breaks(&checkpointed);
        assert_eq!(
            decode_source_range_with_line_breaks(&checkpointed, 258, 262, &line_index).unwrap(),
            ("a\n".into(), [65, 66])
        );

        let mut checkpointed_pair = vec![0xff, 0xfe];
        checkpointed_pair.extend("a".repeat(127).encode_utf16().flat_map(u16::to_le_bytes));
        checkpointed_pair.extend("𝄞b\nc".encode_utf16().flat_map(u16::to_le_bytes));
        let line_index = source_line_breaks(&checkpointed_pair);
        assert_eq!(
            decode_source_range_with_line_breaks(&checkpointed_pair, 260, 264, &line_index)
                .unwrap(),
            ("b\n".into(), [1, 2])
        );

        let mut supplementary = vec![0xff, 0xfe];
        supplementary.extend("A𝄞Z".encode_utf16().flat_map(u16::to_le_bytes));
        let line_index = source_line_breaks(&supplementary);
        assert!(decode_source_range_with_line_breaks(&supplementary, 4, 6, &line_index).is_err());
        assert!(decode_source_range_with_line_breaks(&supplementary, 6, 8, &line_index).is_err());

        let invalid_prefix = [0xff, 0xfe, b'a', 0, 0x00, 0xd8, b'b', 0];
        assert!(decode_source_range(&invalid_prefix, 6, 8).is_err());
    }

    #[test]
    fn vector_top_k_keeps_highest_scores_with_deterministic_ties() {
        let mut top = std::collections::BinaryHeap::new();
        for (chunk_id, score) in [("z", 0.2), ("b", 0.9), ("a", 0.9), ("c", 0.4)] {
            push_top_vector_score(&mut top, chunk_id, score, 3);
        }
        let mut top = top.into_vec();
        top.sort_by(vector_score_order);
        assert_eq!(
            top.into_iter()
                .map(|item| item.chunk_id)
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn vector_decoder_rejects_non_finite_values() {
        let mut bytes = vec![0; VECTOR_DIMENSION * 4];
        bytes[..4].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(decode_vector(&bytes).is_none());
    }
}
