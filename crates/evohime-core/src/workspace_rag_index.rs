use super::*;

pub fn index_workspace(
    connection: &mut Connection,
    workspace_root: &Path,
    config: &IndexConfig,
    rebuild: bool,
    cancelled: impl Fn() -> bool,
    mut progress: impl FnMut(IndexProgress),
) -> Result<IndexSummary, RagError> {
    config.validate()?;
    let started = Instant::now();
    let canonical_root = workspace_root
        .canonicalize()
        .map_err(|error| RagError::InvalidWorkspace(error.to_string()))?;
    if !canonical_root.is_dir() {
        return Err(RagError::InvalidWorkspace(
            "workspace is not a directory".into(),
        ));
    }
    let key = workspace_key(&canonical_root)?;
    let now = now_ms();
    connection.execute(
        "UPDATE workspace_index_runs
         SET status = 'failed', finished_at = ?2, error_summary = '[\"core_restarted\"]'
         WHERE workspace_key = ?1 AND status = 'running'",
        params![key, now],
    )?;
    let generation: i64 = connection.query_row(
        "SELECT COALESCE(MAX(generation), 0) + 1 FROM workspace_index_runs WHERE workspace_key = ?1",
        [&key],
        |row| row.get(0),
    )?;
    let run_id = format!(
        "index-{}-{}",
        generation,
        &sha256_hex(format!("{key}:{now}").as_bytes())[..16]
    );
    connection.execute(
        "INSERT INTO workspace_index_runs
         (run_id, workspace_key, generation, status, started_at, scanner_version,
          chunker_version, tokenizer_version, dirty)
         VALUES (?1, ?2, ?3, 'running', ?4, ?5, ?6, ?7, 1)",
        params![
            run_id,
            key,
            generation,
            now,
            SCANNER_VERSION,
            CHUNKER_VERSION,
            TOKENIZER_VERSION
        ],
    )?;

    let outcome = (|| -> Result<IndexSummary, RagError> {
        let (files, initial_excluded) = collect_files(&canonical_root, config)?;
        let active_generation = active_generation(connection, &key)?;
        let mut indexed = 0usize;
        let mut reused = 0usize;
        let mut chunks = 0usize;
        let mut excluded = initial_excluded;
        let mut errors = Vec::new();
        let mut last_progress = Instant::now()
            .checked_sub(Duration::from_millis(config.progress_interval_ms))
            .unwrap_or_else(Instant::now);

        for (scanned, path) in files.iter().enumerate() {
            if cancelled() {
                return Err(RagError::Cancelled);
            }
            if started.elapsed() > Duration::from_millis(config.run_timeout_ms) {
                return Err(RagError::Timeout);
            }
            let relative = path
                .strip_prefix(&canonical_root)
                .map_err(|_| RagError::Sandbox("indexed path escaped workspace".into()))?
                .to_string_lossy()
                .replace('\\', "/");
            let Some((language, mime)) = language_for(path) else {
                excluded += 1;
                continue;
            };
            let snapshot = match stable_read(&canonical_root, path, config) {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => {
                    excluded += 1;
                    continue;
                }
                Err(error) => return Err(error),
            };
            if snapshot.decode_status == "lossy"
                && matches!(
                    language,
                    "rust" | "typescript" | "javascript" | "json" | "toml" | "yaml"
                )
            {
                errors.push(format!("{relative}:invalid_structured_encoding"));
                excluded += 1;
                continue;
            }
            let file_hash = sha256_hex(&snapshot.bytes);
            let reused_document = if !rebuild {
                active_generation
                    .and_then(|active| {
                        copy_unchanged_document(
                            connection,
                            &key,
                            active,
                            generation,
                            &relative,
                            &file_hash,
                            snapshot.modified_ms,
                        )
                        .transpose()
                    })
                    .transpose()?
            } else {
                None
            };
            if let Some(reused_chunks) = reused_document {
                indexed += 1;
                reused += 1;
                chunks += reused_chunks;
            } else {
                let pending = PendingDocument {
                    relative_path: relative.clone(),
                    language: language.into(),
                    mime: mime.into(),
                    file_hash,
                    size_bytes: snapshot.bytes.len() as u64,
                    encoding: snapshot.encoding.into(),
                    decode_status: snapshot.decode_status.into(),
                    last_modified: snapshot.modified_ms,
                    chunks: chunk_document(path, language, &snapshot, config),
                };
                if chunks + pending.chunks.len() > config.max_chunks_per_run {
                    return Err(RagError::InvalidConfig(
                        "index run chunk budget exceeded".into(),
                    ));
                }
                chunks += pending.chunks.len();
                insert_document(connection, &key, generation, &pending)?;
                indexed += 1;
            }
            if last_progress.elapsed() >= Duration::from_millis(config.progress_interval_ms) {
                progress(IndexProgress {
                    run_id: run_id.clone(),
                    phase: "indexing".into(),
                    scanned_files: scanned + 1,
                    indexed_files: indexed,
                    chunks,
                    excluded,
                });
                last_progress = Instant::now();
            }
        }

        assert_generation_consistent(connection, &key, generation)?;
        publish_generation(PublishGenerationInput {
            connection,
            run_id: &run_id,
            workspace_key: &key,
            generation,
            files: indexed,
            chunks,
            excluded,
            errors: &errors,
        })?;
        gc_superseded_generations(connection, &key)?;
        progress(IndexProgress {
            run_id: run_id.clone(),
            phase: "published".into(),
            scanned_files: files.len(),
            indexed_files: indexed,
            chunks,
            excluded,
        });
        Ok(IndexSummary {
            run_id: run_id.clone(),
            workspace_key: key.clone(),
            generation,
            status: "published".into(),
            indexed_files: indexed,
            reused_files: reused,
            chunks,
            excluded,
            errors,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    })();

    if let Err(error) = &outcome {
        let status = if matches!(error, RagError::Cancelled) {
            "cancelled"
        } else {
            "failed"
        };
        let safe = bounded_error("run", &error.to_string());
        connection.execute(
            "UPDATE workspace_index_runs SET status = ?2, finished_at = ?3,
             error_count = 1, error_summary = json_array(?4), dirty = 1
             WHERE run_id = ?1 AND status = 'running'",
            params![run_id, status, now_ms(), safe],
        )?;
        cleanup_generation(connection, &key, generation)?;
        progress(IndexProgress {
            run_id: run_id.clone(),
            phase: status.into(),
            scanned_files: 0,
            indexed_files: 0,
            chunks: 0,
            excluded: 0,
        });
    }
    outcome
}

pub fn active_generation(
    connection: &Connection,
    workspace_key: &str,
) -> Result<Option<i64>, RagError> {
    Ok(connection
        .query_row(
            "SELECT generation FROM workspace_index_runs
             WHERE workspace_key = ?1 AND status = 'published'",
            [workspace_key],
            |row| row.get(0),
        )
        .optional()?)
}

fn copy_unchanged_document(
    connection: &Connection,
    workspace_key: &str,
    old_generation: i64,
    new_generation: i64,
    path: &str,
    file_hash: &str,
    modified_ms: i64,
) -> Result<Option<usize>, RagError> {
    let document = connection
        .query_row(
            "SELECT document_id, language, mime, size_bytes, encoding, decode_status,
                    last_modified, redaction_status, is_secret_path
             FROM workspace_documents
             WHERE workspace_key = ?1 AND generation = ?2 AND path = ?3
               AND file_hash = ?4 AND status = 'active'",
            params![workspace_key, old_generation, path, file_hash],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            },
        )
        .optional()?;
    let Some((
        old_id,
        language,
        mime,
        size,
        encoding,
        decode_status,
        _old_modified,
        redaction,
        secret,
    )) = document
    else {
        return Ok(None);
    };
    let new_id = stable_id(workspace_key, new_generation, path, "document");
    connection.execute(
        "INSERT INTO workspace_documents
         (document_id, workspace_key, path, generation, language, mime, file_hash,
          size_bytes, encoding, decode_status, last_modified, indexed_at, status,
          redaction_status, is_secret_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                 'active', ?13, ?14)",
        params![
            new_id,
            workspace_key,
            path,
            new_generation,
            language,
            mime,
            file_hash,
            size,
            encoding,
            decode_status,
            modified_ms,
            now_ms(),
            redaction,
            secret
        ],
    )?;
    let mut statement = connection.prepare(
        "SELECT ordinal, chunk_hash, byte_start, byte_end, line_start, line_end,
                parent_context, chunk_text, symbol, symbol_normalized, token_count,
                byte_count, strategy_version
         FROM document_chunks WHERE document_id = ?1 ORDER BY ordinal",
    )?;
    let rows = statement
        .query_map([old_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
                row.get::<_, String>(12)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for row in &rows {
        let chunk_id = stable_id(
            workspace_key,
            new_generation,
            &format!("{path}:{}", row.0),
            "chunk",
        );
        connection.execute(
            "INSERT INTO document_chunks
             (chunk_id, document_id, workspace_key, generation, ordinal, chunk_hash,
              byte_start, byte_end, line_start, line_end, parent_context, chunk_text,
              symbol, symbol_normalized, token_count, byte_count, strategy_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                     ?13, ?14, ?15, ?16, ?17)",
            params![
                chunk_id,
                new_id,
                workspace_key,
                new_generation,
                row.0,
                row.1,
                row.2,
                row.3,
                row.4,
                row.5,
                row.6,
                row.7,
                row.8,
                row.9,
                row.10,
                row.11,
                row.12
            ],
        )?;
        insert_fts(FtsInsertInput {
            connection,
            chunk_id: &chunk_id,
            workspace_key,
            generation: new_generation,
            text: &row.7,
            symbol: &row.9,
            path,
            parent: &row.6,
        })?;
    }
    Ok(Some(rows.len()))
}

fn insert_document(
    connection: &Connection,
    workspace_key: &str,
    generation: i64,
    document: &PendingDocument,
) -> Result<(), RagError> {
    let document_id = stable_id(
        workspace_key,
        generation,
        &document.relative_path,
        "document",
    );
    connection.execute(
        "INSERT INTO workspace_documents
         (document_id, workspace_key, path, generation, language, mime, file_hash,
          size_bytes, encoding, decode_status, last_modified, indexed_at, status,
          redaction_status, is_secret_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                 'active', 'none', 0)",
        params![
            document_id,
            workspace_key,
            document.relative_path,
            generation,
            document.language,
            document.mime,
            document.file_hash,
            document.size_bytes as i64,
            document.encoding,
            document.decode_status,
            document.last_modified,
            now_ms()
        ],
    )?;
    for chunk in &document.chunks {
        let chunk_id = stable_id(
            workspace_key,
            generation,
            &format!("{}:{}", document.relative_path, chunk.ordinal),
            "chunk",
        );
        connection.execute(
            "INSERT INTO document_chunks
             (chunk_id, document_id, workspace_key, generation, ordinal, chunk_hash,
              byte_start, byte_end, line_start, line_end, parent_context, chunk_text,
              symbol, symbol_normalized, token_count, byte_count, strategy_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                     ?13, ?14, ?15, ?16, ?17)",
            params![
                chunk_id,
                document_id,
                workspace_key,
                generation,
                chunk.ordinal as i64,
                chunk.chunk_hash,
                chunk.byte_start as i64,
                chunk.byte_end as i64,
                chunk.line_start as i64,
                chunk.line_end as i64,
                chunk.parent_context,
                chunk.text,
                chunk.symbol,
                chunk.symbol_normalized,
                estimate_tokens(&chunk.text) as i64,
                chunk.text.len() as i64,
                CHUNKER_VERSION
            ],
        )?;
        insert_fts(FtsInsertInput {
            connection,
            chunk_id: &chunk_id,
            workspace_key,
            generation,
            text: &chunk.text,
            symbol: &chunk.symbol_normalized,
            path: &document.relative_path,
            parent: &chunk.parent_context,
        })?;
    }
    Ok(())
}

struct FtsInsertInput<'a> {
    connection: &'a Connection,
    chunk_id: &'a str,
    workspace_key: &'a str,
    generation: i64,
    text: &'a str,
    symbol: &'a str,
    path: &'a str,
    parent: &'a str,
}

fn insert_fts(input: FtsInsertInput<'_>) -> Result<(), RagError> {
    let FtsInsertInput {
        connection,
        chunk_id,
        workspace_key,
        generation,
        text,
        symbol,
        path,
        parent,
    } = input;
    connection.execute(
        "INSERT INTO workspace_chunks_fts
         (chunk_text, symbol_normalized, path, parent_context, chunk_id, workspace_key, generation)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            text,
            symbol,
            path,
            parent,
            chunk_id,
            workspace_key,
            generation
        ],
    )?;
    Ok(())
}

fn assert_generation_consistent(
    connection: &Connection,
    workspace_key: &str,
    generation: i64,
) -> Result<(), RagError> {
    let chunks: i64 = connection.query_row(
        "SELECT COUNT(*) FROM document_chunks WHERE workspace_key = ?1 AND generation = ?2",
        params![workspace_key, generation],
        |row| row.get(0),
    )?;
    let fts: i64 = connection.query_row(
        "SELECT COUNT(*) FROM workspace_chunks_fts WHERE workspace_key = ?1 AND generation = ?2",
        params![workspace_key, generation],
        |row| row.get(0),
    )?;
    let orphans: i64 = connection.query_row(
        "SELECT COUNT(*) FROM document_chunks c
         LEFT JOIN workspace_documents d ON d.document_id = c.document_id
         WHERE c.workspace_key = ?1 AND c.generation = ?2 AND d.document_id IS NULL",
        params![workspace_key, generation],
        |row| row.get(0),
    )?;
    if chunks != fts || orphans != 0 {
        return Err(RagError::InvalidConfig(format!(
            "index consistency failed: chunks={chunks}, fts={fts}, orphans={orphans}"
        )));
    }
    Ok(())
}

struct PublishGenerationInput<'a> {
    connection: &'a mut Connection,
    run_id: &'a str,
    workspace_key: &'a str,
    generation: i64,
    files: usize,
    chunks: usize,
    excluded: usize,
    errors: &'a [String],
}

fn publish_generation(input: PublishGenerationInput<'_>) -> Result<(), RagError> {
    let PublishGenerationInput {
        connection,
        run_id,
        workspace_key,
        generation,
        files,
        chunks,
        excluded,
        errors,
    } = input;
    let transaction = connection.transaction()?;
    let current: String = transaction.query_row(
        "SELECT status FROM workspace_index_runs WHERE run_id = ?1 AND generation = ?2",
        params![run_id, generation],
        |row| row.get(0),
    )?;
    if current != "running" {
        return Err(RagError::Cancelled);
    }
    let newer: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM workspace_index_runs
         WHERE workspace_key = ?1 AND generation > ?2 AND status IN ('running','published')",
        params![workspace_key, generation],
        |row| row.get(0),
    )?;
    if newer != 0 {
        return Err(RagError::Cancelled);
    }
    transaction.execute(
        "UPDATE workspace_index_runs SET status = 'superseded'
         WHERE workspace_key = ?1 AND status = 'published'",
        [workspace_key],
    )?;
    transaction.execute(
        "UPDATE workspace_index_runs
         SET status = 'published', finished_at = ?2, published_at = ?2,
             file_count = ?3, chunk_count = ?4, excluded_count = ?5,
             error_count = ?6, error_summary = ?7, dirty = 0
         WHERE run_id = ?1 AND status = 'running'",
        params![
            run_id,
            now_ms(),
            files as i64,
            chunks as i64,
            excluded as i64,
            errors.len() as i64,
            match serde_json::to_string(errors) {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(%error, "RAG error list serialization failed");
                    "[]".into()
                }
            }
        ],
    )?;
    transaction.commit()?;
    Ok(())
}

fn cleanup_generation(
    connection: &Connection,
    workspace_key: &str,
    generation: i64,
) -> Result<(), RagError> {
    connection.execute(
        "DELETE FROM workspace_chunks_fts WHERE workspace_key = ?1 AND generation = ?2",
        params![workspace_key, generation],
    )?;
    connection.execute(
        "DELETE FROM document_chunks WHERE workspace_key = ?1 AND generation = ?2",
        params![workspace_key, generation],
    )?;
    connection.execute(
        "DELETE FROM workspace_documents WHERE workspace_key = ?1 AND generation = ?2",
        params![workspace_key, generation],
    )?;
    Ok(())
}

fn gc_superseded_generations(connection: &Connection, workspace_key: &str) -> Result<(), RagError> {
    let keep: Option<i64> = connection.query_row(
        "SELECT MAX(generation) FROM workspace_index_runs
             WHERE workspace_key = ?1 AND status = 'superseded'",
        [workspace_key],
        |row| row.get(0),
    )?;
    let mut statement = connection.prepare(
        "SELECT generation FROM workspace_index_runs
         WHERE workspace_key = ?1 AND status IN ('superseded','failed','cancelled')
           AND generation != COALESCE(?2, -1)",
    )?;
    let generations = statement
        .query_map(params![workspace_key, keep], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for generation in generations {
        cleanup_generation(connection, workspace_key, generation)?;
    }
    // Vector retention mirrors the index policy: one published and at most
    // one deprecated generation. Failed/cancelled staging rows have no blobs.
    let keep_vector: Option<String> = connection
        .query_row(
            "SELECT index_id FROM workspace_vector_indexes
             WHERE workspace_key = ?1 AND build_status = 'deprecated'
             ORDER BY published_at DESC LIMIT 1",
            [workspace_key],
            |row| row.get(0),
        )
        .optional()?;
    connection.execute(
        "DELETE FROM workspace_vector_indexes
         WHERE workspace_key = ?1 AND build_status IN ('deprecated','failed','cancelled')
           AND index_id != COALESCE(?2, '')",
        params![workspace_key, keep_vector],
    )?;
    Ok(())
}

pub fn get_index_status(
    connection: &Connection,
    workspace_root: &Path,
) -> Result<IndexStatus, RagError> {
    let key = workspace_key(workspace_root)?;
    let row = connection
        .query_row(
            "SELECT generation, status, file_count, chunk_count, excluded_count, dirty, published_at
             FROM workspace_index_runs WHERE workspace_key = ?1
             ORDER BY CASE status WHEN 'running' THEN 0 WHEN 'published' THEN 1 ELSE 2 END,
                      generation DESC LIMIT 1",
            [&key],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?, row.get::<_, i64>(4)?, row.get::<_, i64>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                ))
            },
        )
        .optional()?;
    let vector = connection
        .query_row(
            "SELECT index_id FROM workspace_vector_indexes
             WHERE workspace_key = ?1 AND build_status = 'published'",
            [&key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(match row {
        Some((generation, status, files, chunks, excluded, dirty, published_at)) => IndexStatus {
            workspace_key: key,
            generation: Some(generation),
            status,
            indexed_files: files as usize,
            chunks: chunks as usize,
            excluded: excluded as usize,
            dirty: dirty != 0,
            published_at,
            vector_mode: if vector.is_some() {
                "hybrid".into()
            } else {
                "fts5".into()
            },
            vector_index_id: vector,
        },
        None => IndexStatus {
            workspace_key: key,
            generation: None,
            status: "not_indexed".into(),
            indexed_files: 0,
            chunks: 0,
            excluded: 0,
            dirty: true,
            published_at: None,
            vector_mode: "fts5".into(),
            vector_index_id: None,
        },
    })
}
