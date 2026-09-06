use super::*;

pub fn build_evidence_context(
    connection: &Connection,
    workspace_root: &Path,
    search: &SearchResult,
    token_budget: usize,
    chunk_count_limit: usize,
    min_chunk_size_tokens: usize,
) -> Result<ContextBuildResult, RagError> {
    if token_budget == 0 || chunk_count_limit == 0 {
        return Ok(ContextBuildResult {
            ledger_id: format!(
                "rag-ledger-{}",
                &sha256_hex(search.query_id.as_bytes())[..20]
            ),
            model_context: String::new(),
            selected_block_ids: Vec::new(),
            citations: Vec::new(),
            rejected: vec!["empty_budget".into()],
            degraded: false,
            estimated_tokens: 0,
        });
    }
    if chunk_count_limit > 64 || token_budget > 128_000 || min_chunk_size_tokens > token_budget {
        return Err(RagError::InvalidConfig(
            "context budget is outside hard limits".into(),
        ));
    }
    let canonical_root = workspace_root.canonicalize()?;
    let ledger_id = format!(
        "rag-ledger-{}",
        &sha256_hex(format!("{}:{}", search.query_id, now_ms()).as_bytes())[..24]
    );
    let mut candidates = search.evidence.clone();
    candidates.sort_by(|left, right| {
        let left_score = left.score + left.checker_confidence;
        let right_score = right.score + right.checker_confidence;
        right_score
            .partial_cmp(&left_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.chunk_id.cmp(&right.chunk_id))
    });
    let mut selected = Vec::new();
    let mut citations = Vec::new();
    let mut rejected = Vec::new();
    let mut context = String::new();
    let mut used_tokens = 0usize;
    let created_at = now_ms();
    for (rank, block) in candidates.iter().enumerate() {
        if selected.len() >= chunk_count_limit {
            rejected.push(format!("{}:chunk_count_limit", block.chunk_id));
            break;
        }
        if block.stale || block.redaction_status == "full" || block.content.is_none() {
            rejected.push(format!("{}:stale_or_redacted", block.chunk_id));
            continue;
        }
        let current_path = canonical_root.join(&block.relative_path);
        let initial = validate_source(
            &canonical_root,
            &current_path,
            &block.content_hash,
            fs::metadata(&current_path)
                .map(|metadata| metadata.len())
                .unwrap_or_default(),
            block.byte_start as usize,
            block.byte_end as usize,
        );
        if initial.is_err() {
            rejected.push(format!("{}:sandbox_or_stale", block.chunk_id));
            continue;
        }
        let snippet = with_parent_context(&current_path, block)?;
        let snippet_tokens = estimate_tokens(&snippet);
        let remaining = token_budget.saturating_sub(used_tokens);
        if remaining < min_chunk_size_tokens || snippet_tokens > remaining {
            rejected.push(format!("{}:budget_exhausted", block.chunk_id));
            break;
        }
        let citation = Citation {
            citation_format_version: CITATION_FORMAT_VERSION,
            id: block.chunk_id.clone(),
            path: block.relative_path.clone(),
            line_range: block.lines,
            chunk_hash: block.chunk_hash.clone(),
            status: CitationStatus::Valid,
            reason: "ranked_evidence".into(),
        };
        context.push_str(&citation.compact());
        context.push('\n');
        context.push_str(&snippet);
        context.push_str("\n\n");
        used_tokens += snippet_tokens + estimate_tokens(&citation.compact());
        selected.push(block.chunk_id.clone());
        citations.push(citation.clone());
        write_rag_ledger(RagLedgerInput {
            connection,
            ledger_id: &ledger_id,
            query_id: &search.query_id,
            block,
            rank: rank + 1,
            snippet: &snippet,
            citation: &citation,
            reread_result: "initial_valid",
            error_code: None,
            created_at,
        })?;
    }
    Ok(ContextBuildResult {
        ledger_id,
        model_context: context,
        selected_block_ids: selected,
        citations,
        rejected,
        degraded: false,
        estimated_tokens: used_tokens,
    })
}

fn with_parent_context(path: &Path, block: &RetrievedChunk) -> Result<String, RagError> {
    let bytes = fs::read(path)?;
    let (text, _, _) = decode_text(&bytes);
    let lines = text.lines().collect::<Vec<_>>();
    let range = block.lines.unwrap_or([1, 1]);
    let logical = block.symbol.is_some();
    let radius = if logical { 2 } else { 3 };
    let start = range[0].saturating_sub(1 + radius).min(lines.len() as u64) as usize;
    let end = (range[1] + radius).min(lines.len() as u64) as usize;
    let mut result = String::new();
    result.push_str(&format!(
        "<source path=\"{}\" parent=\"{}\">\n",
        block.relative_path, block.parent_context
    ));
    for (index, line) in lines[start..end].iter().enumerate() {
        result.push_str(&format!("{}: {}\n", start + index + 1, line));
    }
    result.push_str("</source>");
    Ok(result)
}

/// Final atomic re-read immediately before answer rendering. Updated text and
/// metadata are accepted together; stale evidence is removed from the model
/// context and cannot retain a `valid` citation.
pub fn finalize_citations(
    connection: &Connection,
    workspace_root: &Path,
    search: &SearchResult,
    mut context: ContextBuildResult,
) -> Result<ContextBuildResult, RagError> {
    let root = workspace_root.canonicalize()?;
    let selected = search
        .evidence
        .iter()
        .filter(|block| context.selected_block_ids.contains(&block.chunk_id))
        .map(|block| (block.chunk_id.clone(), block))
        .collect::<HashMap<_, _>>();
    let mut valid_context = String::new();
    let mut stale = 0usize;
    for citation in &mut context.citations {
        let Some(block) = selected.get(&citation.id).copied() else {
            citation.status = CitationStatus::Stale;
            citation.reason = "missing_selected_block".into();
            stale += 1;
            continue;
        };
        let path = root.join(&block.relative_path);
        let final_result = fs::read(&path).map_err(RagError::from).and_then(|bytes| {
            let hash = sha256_hex(&bytes);
            if hash == block.content_hash {
                Ok((
                    bytes,
                    CitationStatus::Valid,
                    block.chunk_hash.clone(),
                    block.lines,
                ))
            } else {
                relocate_nearby(&bytes, block)
                    .map(|(hash, lines)| (bytes, CitationStatus::Updated, hash, Some(lines)))
            }
        });
        match final_result {
            Ok((_bytes, status, chunk_hash, lines)) => {
                citation.status = status;
                citation.chunk_hash = chunk_hash;
                citation.line_range = lines;
                citation.reason = if citation.status == CitationStatus::Updated {
                    "reread_updated".into()
                } else {
                    "reread_valid".into()
                };
                let refreshed = if citation.status == CitationStatus::Updated {
                    let mut refreshed = block.clone();
                    refreshed.lines = lines;
                    with_parent_context(&path, &refreshed)?
                } else {
                    with_parent_context(&path, block)?
                };
                valid_context.push_str(&citation.compact());
                valid_context.push('\n');
                valid_context.push_str(&refreshed);
                valid_context.push_str("\n\n");
                update_rag_ledger(
                    connection,
                    &context.ledger_id,
                    &citation.id,
                    citation,
                    "reread_valid",
                    None,
                )?;
            }
            Err(error) => {
                citation.status = CitationStatus::Stale;
                citation.reason = "reread_failed".into();
                stale += 1;
                let code = bounded_error(&citation.path, &error.to_string());
                update_rag_ledger(
                    connection,
                    &context.ledger_id,
                    &citation.id,
                    citation,
                    "reread_stale",
                    Some(&code),
                )?;
            }
        }
    }
    context.model_context = valid_context;
    context.selected_block_ids = context
        .citations
        .iter()
        .filter(|citation| citation.status != CitationStatus::Stale)
        .map(|citation| citation.id.clone())
        .collect();
    context.estimated_tokens = estimate_tokens(&context.model_context);
    context.degraded = !context.citations.is_empty() && stale * 2 > context.citations.len();
    if context.degraded {
        context.rejected.push("stale_majority".into());
    }
    Ok(context)
}

fn relocate_nearby(bytes: &[u8], block: &RetrievedChunk) -> Result<(String, [u64; 2]), RagError> {
    let (text, _, _) = decode_text(bytes);
    let needle = block.content.as_deref().unwrap_or_default().trim();
    if needle.is_empty() {
        return Err(RagError::InvalidWorkspace("stale empty chunk".into()));
    }
    let Some(byte_start) = text.find(needle) else {
        return Err(RagError::InvalidWorkspace("stale chunk not found".into()));
    };
    let new_line = byte_to_line(&text, byte_start);
    let old_line = block.lines.map(|range| range[0]).unwrap_or(new_line);
    if new_line.abs_diff(old_line) > 5 {
        return Err(RagError::InvalidWorkspace(
            "chunk moved beyond reread window".into(),
        ));
    }
    let payload = format!(
        "{CHUNKER_VERSION}\n{}\n{}\n{needle}",
        block.language, block.parent_context
    );
    Ok((
        sha256_hex(payload.as_bytes()),
        [new_line, byte_to_line(&text, byte_start + needle.len())],
    ))
}

struct RagLedgerInput<'a> {
    connection: &'a Connection,
    ledger_id: &'a str,
    query_id: &'a str,
    block: &'a RetrievedChunk,
    rank: usize,
    snippet: &'a str,
    citation: &'a Citation,
    reread_result: &'a str,
    error_code: Option<&'a str>,
    created_at: i64,
}

fn write_rag_ledger(input: RagLedgerInput<'_>) -> Result<(), RagError> {
    let RagLedgerInput {
        connection,
        ledger_id,
        query_id,
        block,
        rank,
        snippet,
        citation,
        reread_result,
        error_code,
        created_at,
    } = input;
    connection.execute(
        "INSERT INTO rag_context_ledger
         (ledger_id, query_id, block_id, rank, retrieval_score, checker_confidence,
          chunk_hash, snippet_hash, path, line_start, line_end, citation_status,
          selection_reason, reread_result, error_code, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                 ?13, ?14, ?15, ?16)",
        params![
            ledger_id,
            query_id,
            block.chunk_id,
            rank as i64,
            block.score,
            block.checker_confidence,
            citation.chunk_hash,
            sha256_hex(snippet.as_bytes()),
            citation.path,
            citation.line_range.map(|range| range[0] as i64),
            citation.line_range.map(|range| range[1] as i64),
            citation.status.as_str(),
            citation.reason,
            reread_result,
            error_code,
            created_at
        ],
    )?;
    Ok(())
}

fn update_rag_ledger(
    connection: &Connection,
    ledger_id: &str,
    block_id: &str,
    citation: &Citation,
    reread_result: &str,
    error_code: Option<&str>,
) -> Result<(), RagError> {
    connection.execute(
        "UPDATE rag_context_ledger SET chunk_hash = ?3, line_start = ?4,
         line_end = ?5, citation_status = ?6, selection_reason = ?7,
         reread_result = ?8, error_code = ?9
         WHERE ledger_id = ?1 AND block_id = ?2",
        params![
            ledger_id,
            block_id,
            citation.chunk_hash,
            citation.line_range.map(|range| range[0] as i64),
            citation.line_range.map(|range| range[1] as i64),
            citation.status.as_str(),
            citation.reason,
            reread_result,
            error_code
        ],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RagLedgerProjection {
    pub ledger_id: String,
    pub query_id: String,
    pub block_id: String,
    pub rank: usize,
    pub chunk_hash: String,
    pub snippet_hash: String,
    pub path: String,
    pub line_range: Option<[u64; 2]>,
    pub citation_status: String,
    pub selection_reason: String,
    pub reread_result: String,
    pub error_code: Option<String>,
}

pub fn rag_ledger_projection(
    connection: &Connection,
    query_id: &str,
    limit: usize,
) -> Result<Vec<RagLedgerProjection>, RagError> {
    if !(1..=100).contains(&limit) {
        return Err(RagError::InvalidConfig(
            "ledger projection limit must be 1..100".into(),
        ));
    }
    let mut statement = connection.prepare(
        "SELECT ledger_id, query_id, block_id, rank, chunk_hash, snippet_hash,
                path, line_start, line_end, citation_status, selection_reason,
                reread_result, error_code
         FROM rag_context_ledger WHERE query_id = ?1 ORDER BY rank LIMIT ?2",
    )?;
    let records = statement
        .query_map(params![query_id, limit as i64], |row| {
            let start = row.get::<_, Option<i64>>(7)?;
            let end = row.get::<_, Option<i64>>(8)?;
            Ok(RagLedgerProjection {
                ledger_id: row.get(0)?,
                query_id: row.get(1)?,
                block_id: row.get(2)?,
                rank: row.get::<_, i64>(3)? as usize,
                chunk_hash: row.get(4)?,
                snippet_hash: row.get(5)?,
                path: row.get(6)?,
                line_range: start.zip(end).map(|(a, b)| [a as u64, b as u64]),
                citation_status: row.get(9)?,
                selection_reason: row.get(10)?,
                reread_result: row.get(11)?,
                error_code: row.get(12)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(records)
}

/// Validates document provenance submitted by Memory Extraction against the
/// currently published RAG generation and a fresh filesystem read. This is
/// the only path that upgrades document evidence from `pending/unknown` to a
/// verified candidate; stale citations remain pending.
pub fn verify_document_provenance(
    connection: &Connection,
    workspace_root: &Path,
    relative_path: &str,
    chunk_hash: &str,
) -> Result<bool, RagError> {
    validate_filters(&QueryFilters {
        path: Some(relative_path.to_string()),
        language: None,
    })?;
    let key = workspace_key(workspace_root)?;
    let Some(generation) = active_generation(connection, &key)? else {
        return Ok(false);
    };
    let source = connection
        .query_row(
            "SELECT d.file_hash, d.size_bytes, c.byte_start, c.byte_end
             FROM document_chunks c
             JOIN workspace_documents d ON d.document_id = c.document_id
             WHERE d.workspace_key = ?1 AND d.generation = ?2 AND d.path = ?3
               AND c.chunk_hash = ?4 AND d.status = 'active'
               AND d.redaction_status != 'full' AND d.is_secret_path = 0",
            params![
                key,
                generation,
                relative_path.replace('\\', "/"),
                chunk_hash
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((file_hash, size, start, end)) = source else {
        return Ok(false);
    };
    let root = workspace_root.canonicalize()?;
    Ok(validate_source(
        &root,
        &root.join(relative_path),
        &file_hash,
        size as u64,
        start as usize,
        end as usize,
    )
    .is_ok())
}


