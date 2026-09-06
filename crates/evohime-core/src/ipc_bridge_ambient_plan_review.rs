use super::*;

impl IpcBridge {
    pub(super) async fn dispatch_plan_review<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        _request_id: String,
        _client_id: String,
        _command_hash: String,
        command: Option<generated::command_envelope::Command>,
    ) -> Result<(), IpcBridgeError> {
        match command {
                Some(generated::command_envelope::Command::StopPlanReview(request)) => {
                    let cancelled = self
                        .review_tasks
                        .lock()
                        .await
                        .get(&request.review_id)
                        .cloned();
                    if let Some(ref token) = cancelled {
                        token.cancel();
                    }
                    self.write_response(
                        writer,
                        "review.stop.accepted",
                        serde_json::to_vec(&serde_json::json!({
                            "review_id": request.review_id,
                            "accepted": cancelled.is_some(),
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ListPlanReviews(request)) => {
                    let limit = (request.limit as usize).clamp(1, 50);
                    let results = self.review_results.lock().await;
                    let mut items: Vec<_> = results.values().cloned().collect();
                    drop(results);
                    if let Ok(events) = self.journal.review_history(limit).await {
                        for event in events {
                            if let Some(result) = review_result_from_event(&event.payload) {
                                if !items.iter().any(|item| item.review_id == result.review_id) {
                                    items.push(result);
                                }
                            }
                        }
                    }
                    items.sort_by(|left, right| left.review_id.cmp(&right.review_id));
                    items.truncate(limit);
                    self.write_response(
                        writer,
                        "review.list",
                        serde_json::to_vec(&serde_json::json!({ "reviews": items }))
                            .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ClearPlanReviewHistory(_)) => {
                    // Running reviews keep their own state; only what the history
                    // lists is dropped, and the marker is what listing reads.
                    self.review_results.lock().await.clear();
                    let marker_id = format!("review-history-{}", self.latest_sequence().await);
                    // Recorded directly rather than published: the shell lists again
                    // as soon as this response arrives, and a marker still travelling
                    // through the coordinator's broadcast would not be in the journal
                    // yet, so that listing would return the reviews just cleared.
                    // Nothing subscribes to the marker, so no push is lost.
                    let _ = self
                        .journal
                        .record(&CoreEvent::ReviewHistoryCleared { marker_id })
                        .await;
                    self.write_response(
                        writer,
                        "review.historyCleared",
                        serde_json::to_vec(&serde_json::json!({ "cleared": true }))
                            .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::GetPlanReview(request)) => {
                    let mut result = self
                        .review_results
                        .lock()
                        .await
                        .get(&request.review_id)
                        .cloned();
                    if result.is_none() {
                        if let Ok(events) = self.journal.task_history(&request.review_id, 10).await
                        {
                            result = events
                                .iter()
                                .rev()
                                .find_map(|event| review_result_from_event(&event.payload));
                        }
                    }
                    self.write_response(
                        writer,
                        "review.result",
                        serde_json::to_vec(&serde_json::json!({
                            "review_id": request.review_id,
                            "result": result,
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ExportPlanReview(request)) => {
                    let mut result = self
                        .review_results
                        .lock()
                        .await
                        .get(&request.review_id)
                        .cloned();
                    if result.is_none() {
                        if let Ok(events) = self.journal.task_history(&request.review_id, 10).await
                        {
                            result = events
                                .iter()
                                .rev()
                                .find_map(|event| review_result_from_event(&event.payload));
                        }
                    }
                    let result = result.ok_or_else(|| FrameError::Io("review not found".into()))?;
                    let destination = std::path::PathBuf::from(&request.destination_path);
                    if destination.extension().and_then(|value| value.to_str()) != Some("md") {
                        return Err(
                            FrameError::Io("review export must be a Markdown file".into()).into(),
                        );
                    }
                    let content = if request.include_reviewers {
                        serde_json::to_string_pretty(&result).unwrap_or_default()
                    } else {
                        result.final_markdown.clone()
                    };
                    tokio::fs::write(&destination, content)
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(
                        writer,
                        "review.exported",
                        serde_json::to_vec(&serde_json::json!({
                            "review_id": request.review_id,
                            "destination_path": request.destination_path,
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::RevisePlan(request)) => {
                    self.revise_plan(request, writer).await?;
                }
                Some(generated::command_envelope::Command::StopRevision(request)) => {
                    let cancelled = self
                        .revision_tasks
                        .lock()
                        .await
                        .get(&request.revision_id)
                        .cloned();
                    if let Some(ref token) = cancelled {
                        token.cancel();
                    }
                    self.write_response(
                        writer,
                        "revision.stop.accepted",
                        serde_json::to_vec(&serde_json::json!({
                            "revision_id": request.revision_id,
                            "accepted": cancelled.is_some(),
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::SaveRevisedPlan(request)) => {
                    // Правка переживает перезапуск ядра: обновление Евы перезапускает
                    // Core, а нажать «сохранить» пользователь может и после этого.
                    let mut result = self
                        .revision_results
                        .lock()
                        .await
                        .get(&request.revision_id)
                        .cloned();
                    if result.is_none() {
                        if let Ok(events) =
                            self.journal.task_history(&request.revision_id, 10).await
                        {
                            result = events
                                .iter()
                                .rev()
                                .find_map(|event| revision_result_from_event(&event.payload));
                        }
                    }
                    // Отказ отвечает событием, а не ошибкой кадра: ошибка кадра рвёт
                    // соединение с оболочкой, и опечатка в имени файла выглядела бы
                    // как падение ядра.
                    let failure = match &result {
                        None => Some("правка не найдена: запусти её заново".to_string()),
                        Some(_)
                            if std::path::Path::new(&request.destination_path)
                                .extension()
                                .and_then(|value| value.to_str())
                                != Some("md") =>
                        {
                            Some("сохранить план можно только в файл .md".to_string())
                        }
                        Some(_) => None,
                    };
                    let failure = match (failure, result) {
                        (Some(reason), _) => Some(reason),
                        (None, Some(result)) => {
                            tokio::fs::write(&request.destination_path, &result.revised_markdown)
                                .await
                                .err()
                                .map(|error| error.to_string())
                        }
                        (None, None) => Some("правка не найдена: запусти её заново".to_string()),
                    };
                    match failure {
                        Some(error) => {
                            self.write_response(
                                writer,
                                "plan.save_failed",
                                serde_json::to_vec(&serde_json::json!({
                                    "revision_id": request.revision_id,
                                    "destination_path": request.destination_path,
                                    "error": error,
                                }))
                                .unwrap_or_default(),
                            )
                            .await?;
                        }
                        None => {
                            self.write_response(
                                writer,
                                "plan.saved",
                                serde_json::to_vec(&serde_json::json!({
                                    "revision_id": request.revision_id,
                                    "destination_path": request.destination_path,
                                }))
                                .unwrap_or_default(),
                            )
                            .await?;
                        }
                    }
                }

            _ => unreachable!("command routed to the wrong domain"),
        }
        Ok(())
    }
}
