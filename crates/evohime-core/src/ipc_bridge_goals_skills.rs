impl IpcBridge {
    pub(crate) async fn dispatch_create_goal(
        &self,
        request: generated::CreateGoal,
        command_hash: &str,
    ) -> generated::GoalActionResult {
        let invalid = !valid_goal_token(&request.goal_id)
            || !valid_checkpoint_workspace(&request.workspace_path)
            || (!request.chat_id.is_empty() && !valid_goal_token(&request.chat_id))
            || request.objective.trim().is_empty()
            || request.success_criteria.len() > crate::goal::GOAL_MAX_CRITERIA
            || !valid_goal_token(&request.idempotency_key);
        if invalid {
            return goal_action_error(
                "",
                "create",
                "invalid_argument",
                "Параметры цели отклонены.",
            );
        }
        let criteria = match goal_criteria_from_request(&request.success_criteria) {
            Ok(criteria) if !criteria.is_empty() => criteria,
            _ => {
                return goal_action_error(
                    &request.goal_id,
                    "create",
                    "invalid_argument",
                    "Цель должна содержать хотя бы один критерий.",
                )
            }
        };
        let now = crate::goal::now_ms();
        let goal = crate::goal::GoalV1 {
            id: request.goal_id.clone(),
            version: 1,
            workspace_id: crate::goal::workspace_id_from_path(&request.workspace_path),
            chat_id: (!request.chat_id.is_empty()).then_some(request.chat_id),
            objective: request.objective,
            success_criteria: criteria,
            status: crate::goal::GoalStatus::Active,
            progress_summary: "Цель создана; доказательства ещё не подтверждены.".into(),
            completed_criteria: Vec::new(),
            remaining_criteria: Vec::new(),
            blockers: Vec::new(),
            next_action: Some("Выполнить критерии и подтвердить Core evidence.".into()),
            workflow_run_ids: Vec::new(),
            child_run_ids: Vec::new(),
            checkpoint_id: None,
            token_budget: (request.token_budget > 0).then_some(request.token_budget),
            cost_budget_micros: (request.cost_budget_micros > 0)
                .then_some(request.cost_budget_micros),
            continuation_budget: (request.continuation_budget > 0)
                .then_some(request.continuation_budget),
            created_at_ms: now,
            updated_at_ms: now,
            created_by: "shell".into(),
            updated_by: "shell".into(),
            content_hash: String::new(),
        };
        let runtime = crate::goal::GoalRuntime::new(self.journal.clone());
        let goal_command_hash = crate::research::sha256_hex(command_hash.as_bytes());
        match runtime
            .create(
                &goal,
                crate::goal::GoalCommand::new(
                    "shell",
                    &request.idempotency_key,
                    &goal_command_hash,
                ),
            )
            .await
        {
            Ok(result) => {
                self.notify_goal_event(result.event_sequence);
                goal_action_result_from_mutation(result)
            }
            Err(error) => goal_action_error(
                &request.goal_id,
                "create",
                goal_storage_error_code(&error),
                &goal_storage_error_message(&error),
            ),
        }
    }

    pub(crate) async fn dispatch_get_goal(&self, request: generated::GetGoal) -> generated::GoalProjection {
        if !valid_goal_token(&request.goal_id) {
            return goal_projection_error("", "invalid_argument");
        }
        let runtime = crate::goal::GoalRuntime::new(self.journal.clone());
        match runtime.get(&request.goal_id).await {
            Ok(Some(goal)) => goal_projection(&goal, ""),
            Ok(None) => goal_projection_error(&request.goal_id, "not_found"),
            Err(error) => goal_projection_error(&request.goal_id, goal_storage_error_code(&error)),
        }
    }

    pub(crate) async fn dispatch_list_goals(
        &self,
        request: generated::ListGoals,
    ) -> generated::GoalListProjection {
        let limit = if request.limit == 0 {
            crate::goal::GOAL_MAX_READ_LIMIT
        } else {
            request.limit as usize
        };
        if !valid_checkpoint_workspace(&request.workspace_path)
            || limit > crate::goal::GOAL_MAX_READ_LIMIT
        {
            return generated::GoalListProjection {
                schema_version: crate::goal::GOAL_SCHEMA_VERSION,
                error_code: "invalid_argument".into(),
                ..Default::default()
            };
        }
        let workspace_id = crate::goal::workspace_id_from_path(&request.workspace_path);
        let runtime = crate::goal::GoalRuntime::new(self.journal.clone());
        match (
            runtime.list(&workspace_id, limit).await,
            runtime.recovery(&workspace_id).await,
        ) {
            (Ok(goals), Ok(recovery)) => {
                let warnings = recovery
                    .into_iter()
                    .map(|item| (item.goal_id, item.warning))
                    .collect::<std::collections::HashMap<_, _>>();
                let mut projected_goals = Vec::new();
                let mut projected_bytes = 0usize;
                let mut truncated = false;
                for goal in &goals {
                    let projection = goal_projection(
                        goal,
                        warnings.get(&goal.id).map(String::as_str).unwrap_or(""),
                    );
                    let next_bytes = projected_bytes.saturating_add(projection.encoded_len());
                    if next_bytes > GOAL_LIST_MAX_PROJECTION_BYTES {
                        truncated = true;
                        break;
                    }
                    projected_bytes = next_bytes;
                    projected_goals.push(projection);
                }
                generated::GoalListProjection {
                    schema_version: crate::goal::GOAL_SCHEMA_VERSION,
                    goals: projected_goals,
                    error_code: if truncated {
                        "projection_truncated".into()
                    } else {
                        String::new()
                    },
                    truncated,
                }
            }
            (Err(error), _) | (_, Err(error)) => generated::GoalListProjection {
                schema_version: crate::goal::GOAL_SCHEMA_VERSION,
                error_code: goal_storage_error_code(&error).into(),
                ..Default::default()
            },
        }
    }

    pub(crate) async fn dispatch_goal_transition(
        &self,
        request: generated::GoalAction,
        status: crate::goal::GoalStatus,
        command_hash: &str,
    ) -> generated::GoalActionResult {
        let action = match status {
            crate::goal::GoalStatus::Paused => "pause",
            crate::goal::GoalStatus::Active => "resume",
            crate::goal::GoalStatus::Cancelled => "cancel",
            _ => "transition",
        };
        if !valid_goal_action(
            &request.goal_id,
            request.expected_version,
            &request.idempotency_key,
        ) {
            return goal_action_error(
                &request.goal_id,
                action,
                "invalid_argument",
                "Действие цели отклонено.",
            );
        }
        let runtime = crate::goal::GoalRuntime::new(self.journal.clone());
        let goal_command_hash = crate::research::sha256_hex(command_hash.as_bytes());
        match runtime
            .transition(
                &request.goal_id,
                request.expected_version,
                status,
                crate::goal::GoalCommand::new(
                    "shell",
                    &request.idempotency_key,
                    &goal_command_hash,
                ),
            )
            .await
        {
            Ok(result) => {
                self.notify_goal_event(result.event_sequence);
                goal_action_result_from_mutation(result)
            }
            Err(error) => goal_action_error(
                &request.goal_id,
                action,
                goal_storage_error_code(&error),
                &goal_storage_error_message(&error),
            ),
        }
    }

    pub(crate) async fn dispatch_update_goal(
        &self,
        request: generated::UpdateGoal,
        command_hash: &str,
    ) -> generated::GoalActionResult {
        if !valid_goal_action(
            &request.goal_id,
            request.expected_version,
            &request.idempotency_key,
        ) {
            return goal_action_error(
                &request.goal_id,
                "update",
                "invalid_argument",
                "Обновление цели отклонено.",
            );
        }
        let criteria = if request.success_criteria.is_empty() {
            None
        } else {
            match goal_criteria_from_request(&request.success_criteria) {
                Ok(criteria) => Some(criteria),
                Err(_) => {
                    return goal_action_error(
                        &request.goal_id,
                        "update",
                        "invalid_argument",
                        "Критерии цели отклонены.",
                    )
                }
            }
        };
        let objective = (!request.objective.trim().is_empty()).then_some(request.objective);
        if objective.is_none() && criteria.is_none() {
            return goal_action_error(
                &request.goal_id,
                "update",
                "invalid_argument",
                "Нет изменений для цели.",
            );
        }
        let runtime = crate::goal::GoalRuntime::new(self.journal.clone());
        let goal_command_hash = crate::research::sha256_hex(command_hash.as_bytes());
        match runtime
            .update(
                &request.goal_id,
                request.expected_version,
                objective,
                criteria,
                crate::goal::GoalCommand::new(
                    "shell",
                    &request.idempotency_key,
                    &goal_command_hash,
                ),
            )
            .await
        {
            Ok(result) => {
                self.notify_goal_event(result.event_sequence);
                goal_action_result_from_mutation(result)
            }
            Err(error) => goal_action_error(
                &request.goal_id,
                "update",
                goal_storage_error_code(&error),
                &goal_storage_error_message(&error),
            ),
        }
    }

    pub(crate) async fn dispatch_verify_goal_criterion(
        &self,
        request: generated::VerifyGoalCriterion,
        command_hash: &str,
    ) -> generated::GoalActionResult {
        if !valid_goal_action(
            &request.goal_id,
            request.expected_version,
            &request.idempotency_key,
        ) || !valid_goal_token(&request.criterion_id)
        {
            return goal_action_error(
                &request.goal_id,
                "verify_criterion",
                "invalid_argument",
                "Evidence критерия отклонена.",
            );
        }
        let runtime = crate::goal::GoalRuntime::new(self.journal.clone());
        let goal_command_hash = crate::research::sha256_hex(command_hash.as_bytes());
        let goal = match runtime.get(&request.goal_id).await {
            Ok(Some(goal)) => goal,
            Ok(None) => {
                return goal_action_error(
                    &request.goal_id,
                    "verify_criterion",
                    "not_found",
                    "Цель не найдена.",
                )
            }
            Err(error) => {
                return goal_action_error(
                    &request.goal_id,
                    "verify_criterion",
                    goal_storage_error_code(&error),
                    &goal_storage_error_message(&error),
                )
            }
        };
        let is_manual = goal
            .success_criteria
            .iter()
            .find(|criterion| criterion.id == request.criterion_id)
            .is_some_and(|criterion| criterion.kind == crate::goal::GoalCriterionKind::Manual);
        if !is_manual {
            return goal_action_error(
                &request.goal_id,
                "verify_criterion",
                "authority_denied",
                "Этот критерий подтверждается только Core runtime.",
            );
        }
        let evidence_digest = crate::research::sha256_hex(
            format!(
                "{}:{}:{}",
                request.goal_id, request.criterion_id, goal_command_hash
            )
            .as_bytes(),
        );
        let evidence_ref = format!("core:user-decision:{evidence_digest}");
        match runtime
            .verify_criterion(
                &request.goal_id,
                request.expected_version,
                crate::goal::GoalCriterionEvidence::new(
                    &request.criterion_id,
                    &evidence_ref,
                    "core.user-decision",
                    "goal-v1",
                ),
                crate::goal::GoalCommand::new(
                    "shell",
                    &request.idempotency_key,
                    &goal_command_hash,
                ),
            )
            .await
        {
            Ok(result) => {
                self.notify_goal_event(result.event_sequence);
                goal_action_result_from_mutation(result)
            }
            Err(error) => goal_action_error(
                &request.goal_id,
                "verify_criterion",
                goal_storage_error_code(&error),
                &goal_storage_error_message(&error),
            ),
        }
    }

    pub(crate) async fn dispatch_link_goal_reference(
        &self,
        request: generated::LinkGoalReference,
        command_hash: &str,
    ) -> generated::GoalActionResult {
        if !valid_goal_action(
            &request.goal_id,
            request.expected_version,
            &request.idempotency_key,
        ) || !valid_goal_token(&request.kind)
            || !valid_goal_token(&request.reference_id)
        {
            return goal_action_error(
                &request.goal_id,
                "link_reference",
                "invalid_argument",
                "Ссылка цели отклонена.",
            );
        }
        let runtime = crate::goal::GoalRuntime::new(self.journal.clone());
        let goal_command_hash = crate::research::sha256_hex(command_hash.as_bytes());
        match runtime
            .link_reference(
                &request.goal_id,
                request.expected_version,
                &request.kind,
                &request.reference_id,
                crate::goal::GoalCommand::new(
                    "shell",
                    &request.idempotency_key,
                    &goal_command_hash,
                ),
            )
            .await
        {
            Ok(result) => {
                self.notify_goal_event(result.event_sequence);
                goal_action_result_from_mutation(result)
            }
            Err(error) => goal_action_error(
                &request.goal_id,
                "link_reference",
                goal_storage_error_code(&error),
                &goal_storage_error_message(&error),
            ),
        }
    }

    pub(crate) fn notify_goal_event(&self, sequence: i64) {
        if let Some(coordinator) = &self.coordinator {
            coordinator.notify_journalled(sequence.max(0) as u64);
        }
    }

    pub(crate) async fn dispatch_list_skills<W: AsyncWrite + Unpin>(
        &self,
        request: generated::ListSkills,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        let workspace = match validate_skill_workspace(&request.workspace_path) {
            Ok(workspace) => workspace,
            Err(error) => {
                return self
                    .write_skill_catalog(
                        writer,
                        generated::SkillCatalogProjection {
                            schema_version: crate::skill_registry::SKILL_SCHEMA_VERSION,
                            diagnostics: vec![generated::SkillDiagnosticProjection {
                                code: error.code().into(),
                                message: "Каталог skills недоступен.".into(),
                                ..Default::default()
                            }],
                            ..Default::default()
                        },
                    )
                    .await;
            }
        };
        let mut registry = crate::skill_registry::SkillRegistry::for_workspace(&workspace);
        let catalog = registry.catalog();
        let limit = if request.limit == 0 {
            crate::skill_registry::MAX_SKILLS
        } else {
            (request.limit as usize).min(crate::skill_registry::MAX_SKILLS)
        };
        let projection = generated::SkillCatalogProjection {
            schema_version: catalog.schema_version,
            skills: catalog
                .skills
                .into_iter()
                .take(limit)
                .map(skill_metadata_projection)
                .collect(),
            diagnostics: catalog
                .diagnostics
                .into_iter()
                .take(32)
                .map(skill_diagnostic_projection)
                .collect(),
        };
        self.write_skill_catalog(writer, projection).await
    }

    pub(crate) async fn dispatch_load_skill<W: AsyncWrite + Unpin>(
        &self,
        request: generated::LoadSkill,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        let max_bytes = if request.max_bytes == 0 {
            crate::skill_registry::MAX_SKILL_BYTES
        } else {
            (request.max_bytes as usize).min(crate::skill_registry::MAX_SKILL_BYTES)
        };
        let result = match validate_skill_workspace(&request.workspace_path) {
            Ok(workspace) => {
                let mut registry = crate::skill_registry::SkillRegistry::for_workspace(&workspace);
                match registry.load(&request.skill_id) {
                    Ok(skill) if skill.content.len() <= max_bytes => {
                        generated::SkillContentResult {
                            schema_version: skill.metadata.schema_version,
                            skill_id: skill.metadata.skill_id,
                            version: skill.metadata.version,
                            content: skill.content,
                            content_hash: skill.metadata.content_hash,
                            source_ref: skill.metadata.source_ref,
                            cache_hit: skill.cache_hit,
                            ..Default::default()
                        }
                    }
                    Ok(_) => skill_content_error(&request.skill_id, "too_large"),
                    Err(error) => skill_content_error(&request.skill_id, error.code()),
                }
            }
            Err(error) => skill_content_error(&request.skill_id, error.code()),
        };
        if result.error_code.is_empty() {
            self.append_skill_trace(
                &result.skill_id,
                "skill.loaded",
                serde_json::json!({
                    "skill_id": result.skill_id,
                    "version": result.version,
                    "content_hash": result.content_hash,
                    "source_ref": result.source_ref,
                }),
            )
            .await;
        }
        self.write_skill_content(writer, result).await
    }

    pub(crate) async fn dispatch_load_skill_reference<W: AsyncWrite + Unpin>(
        &self,
        request: generated::LoadSkillReference,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        let max_bytes = if request.max_bytes == 0 {
            crate::skill_registry::MAX_REFERENCE_BYTES
        } else {
            (request.max_bytes as usize).min(crate::skill_registry::MAX_REFERENCE_BYTES)
        };
        let result = match validate_skill_workspace(&request.workspace_path) {
            Ok(workspace) => {
                let mut registry = crate::skill_registry::SkillRegistry::for_workspace(&workspace);
                match registry.load_reference(&request.skill_id, &request.reference) {
                    Ok(reference) if reference.content.len() <= max_bytes => {
                        generated::SkillReferenceResult {
                            schema_version: crate::skill_registry::SKILL_SCHEMA_VERSION,
                            skill_id: request.skill_id.clone(),
                            reference: reference.name,
                            content: reference.content,
                            content_hash: reference.content_hash,
                            source_ref: reference.provenance.source_ref,
                            ..Default::default()
                        }
                    }
                    Ok(_) => {
                        skill_reference_error(&request.skill_id, &request.reference, "too_large")
                    }
                    Err(error) => {
                        skill_reference_error(&request.skill_id, &request.reference, error.code())
                    }
                }
            }
            Err(error) => {
                skill_reference_error(&request.skill_id, &request.reference, error.code())
            }
        };
        if result.error_code.is_empty() {
            self.append_skill_trace(
                &result.skill_id,
                "skill.reference.loaded",
                serde_json::json!({
                    "skill_id": result.skill_id,
                    "reference": result.reference,
                    "content_hash": result.content_hash,
                    "source_ref": result.source_ref,
                }),
            )
            .await;
        }
        self.write_skill_reference(writer, result).await
    }

    pub(crate) async fn append_skill_trace(
        &self,
        skill_id: &str,
        event_type: &str,
        payload: serde_json::Value,
    ) {
        let database = self.journal.database();
        let database = database.lock().await;
        if let Err(error) = database.append_event(
            &format!("skill:{skill_id}"),
            event_type,
            &serde_json::to_vec(&payload).unwrap_or_default(),
        ) {
            tracing::warn!(target = "skill.registry", %error, "skill trace could not be persisted");
        }
    }

    pub(crate) async fn write_skill_catalog<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        projection: generated::SkillCatalogProjection,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: String::new(),
            event_type: "skill.catalog".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::SkillCatalog(projection)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    pub(crate) async fn write_skill_content<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        result: generated::SkillContentResult,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: result.skill_id.clone(),
            event_type: "skill.loaded".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::SkillContent(result)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    pub(crate) async fn write_skill_reference<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        result: generated::SkillReferenceResult,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: result.skill_id.clone(),
            event_type: "skill.reference.loaded".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::SkillReference(result)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    pub(crate) async fn write_task_checkpoint_projection<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        projection: generated::TaskCheckpointProjection,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: projection.task_id.clone(),
            event_type: "task.checkpoint".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::TaskCheckpoint(projection)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    pub(crate) async fn write_task_checkpoint_action_result<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        result: generated::TaskCheckpointActionResult,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: result.task_id.clone(),
            event_type: "task.checkpoint.action".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::TaskCheckpointActionResult(result)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    pub(crate) async fn write_goal_projection<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        projection: generated::GoalProjection,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: projection.goal_id.clone(),
            event_type: "goal.projection".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::Goal(projection)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    pub(crate) async fn write_goal_list_projection<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        projection: generated::GoalListProjection,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: String::new(),
            event_type: "goal.list".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::GoalList(projection)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    pub(crate) async fn write_goal_action_result<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        result: generated::GoalActionResult,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: result.goal_id.clone(),
            event_type: "goal.action".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::GoalAction(result)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    pub(crate) async fn write_continuation_projection<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: serde_json::Value =
            serde_json::from_slice(&payload).map_err(|error| FrameError::Io(error.to_string()))?;
        let number = |name: &str| {
            value
                .get(name)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        };
        let projection = generated::ContinuationProjection {
            schema_version: 1,
            run_id: value
                .get("run_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            owner_scope: value
                .get("owner_scope")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            policy_id: value
                .get("policy_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            policy_revision: number("policy_revision"),
            policy_hash: value
                .get("policy_hash")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            state: value
                .get("state")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            continuation_index: number("continuation_index"),
            max_continuations: number("max_continuations"),
            model_turns: number("used_model_turns"),
            max_model_turns: number("max_model_turns"),
            token_used: number("token_used"),
            cost_used_micros: number("cost_used_micros"),
            stop_reason: value
                .get("stop_reason")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            error_code: value
                .get("error_code")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            gates: value
                .get("gates")
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .take(32)
                        .map(|item| generated::ContinuationGateProjection {
                            gate_id: item
                                .get("gate_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .into(),
                            kind: String::new(),
                            capability_ref: String::new(),
                            status: item
                                .get("status")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .into(),
                            evidence_ref: item
                                .get("evidence_ref")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .into(),
                            error_code: item
                                .get("error_code")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .into(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        };
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: projection.run_id.clone(),
            event_type: "continuation.run".into(),
            payload,
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::Continuation(projection)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    pub(crate) async fn write_continuation_action<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: serde_json::Value =
            serde_json::from_slice(&payload).map_err(|error| FrameError::Io(error.to_string()))?;
        let result = generated::ContinuationActionResult {
            schema_version: 1,
            run_id: value
                .get("run_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            action: value
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            applied: value
                .get("applied")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            deduplicated: value
                .get("deduplicated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            error_code: value
                .get("error_code")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
        };
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: result.run_id.clone(),
            event_type: "continuation.action".into(),
            payload,
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::ContinuationAction(result)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }
}