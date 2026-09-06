    use super::*;

    fn raw(kind: &str, scope: &str, trust: &str, confidence: f64) -> RawCandidate {
        RawCandidate {
            kind: kind.to_owned(),
            statement: "Использовать русский язык в UI".to_owned(),
            scope: scope.to_owned(),
            canonical_subject: "Язык интерфейса".to_owned(),
            model_confidence: confidence,
            verification_confidence: 0.99,
            reason: "пользователь сказал явно".to_owned(),
            evidence_locator: RawEvidenceLocator {
                message_id: "msg-1".to_owned(),
                ..RawEvidenceLocator::default()
            },
            privacy: "normal".to_owned(),
            source_trust: trust.to_owned(),
            suggested_ttl_ms: 0,
        }
    }

    fn candidate(raw: &RawCandidate) -> (Candidate, CanonicalSubject) {
        validate_candidate(raw, &AliasTable::new(), &ExtractionPolicy::default())
            .expect("candidate validates")
    }

    #[test]
    fn model_verification_confidence_is_never_trusted() {
        let (candidate, _) = candidate(&raw("preference", "workspace", "user", 0.9));
        assert_eq!(candidate.verification_confidence, 0.0);
    }

    #[test]
    fn strict_low_risk_user_preference_can_auto_confirm() {
        let source = raw("preference", "workspace", "user", 0.9);
        let (candidate, subject) = candidate(&source);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::AutoConfirm);
        assert_eq!(decision.state, ConfirmationState::Confirmed);
        assert_eq!(decision.risk, RiskClass::Low);
        assert_eq!(decision.ttl_ms, 180 * DAY_MS);
    }

    #[test]
    fn strict_low_risk_below_threshold_falls_back_to_pending() {
        let source = raw("preference", "workspace", "user", 0.84);
        let (candidate, subject) = candidate(&source);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Pending);
        assert_eq!(decision.reason, PolicyReason::ConfidenceBelowThreshold);
    }

    #[test]
    fn model_inference_never_grounds_strict_save() {
        let source = raw("preference", "workspace", "model_inference", 1.0);
        let (candidate, subject) = candidate(&source);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Pending);
        assert_eq!(decision.reason, PolicyReason::UntrustedSource);
    }

    #[test]
    fn constraint_and_decision_always_require_approval() {
        for kind in ["constraint", "decision"] {
            let source = raw(kind, "project", "user", 1.0);
            let (candidate, subject) = candidate(&source);
            let decision = evaluate(
                &candidate,
                &TurnContext::strict_with_trigger("запомни"),
                &subject,
                &ExtractionPolicy::default(),
            );
            assert_eq!(decision.outcome, PolicyOutcome::Pending, "{kind}");
            assert_eq!(decision.reason, PolicyReason::KindRequiresApproval);
            assert_eq!(decision.risk, RiskClass::High);
        }
    }

    #[test]
    fn open_mode_always_produces_pending() {
        let source = raw("preference", "workspace", "user", 1.0);
        let (candidate, subject) = candidate(&source);
        let decision = evaluate(
            &candidate,
            &TurnContext::open(),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.state, ConfirmationState::PendingConfirmation);
        assert_eq!(decision.reason, PolicyReason::OpenModeAlwaysPending);
    }

    #[test]
    fn strict_mode_without_trigger_rejects() {
        let source = raw("preference", "workspace", "user", 1.0);
        let (candidate, subject) = candidate(&source);
        let context = TurnContext {
            mode: ExtractionMode::Strict,
            trigger: None,
            user_asserted: true,
        };
        let decision = evaluate(&candidate, &context, &subject, &ExtractionPolicy::default());
        assert_eq!(decision.outcome, PolicyOutcome::Reject);
        assert_eq!(decision.reason, PolicyReason::NoExplicitTrigger);
    }

    #[test]
    fn secrets_are_rejected_before_persistence() {
        let mut source = raw("preference", "workspace", "user", 1.0);
        source.statement = "ключ доступа sk-live-1234567890".to_owned();
        let (candidate, subject) = candidate(&source);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Reject);
        assert_eq!(decision.reason, PolicyReason::SecretNeverStored);
    }

    #[test]
    fn session_summary_is_never_auto_promoted() {
        let source = raw("session_summary", "session", "user", 1.0);
        let (candidate, subject) = candidate(&source);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Pending);
        assert_eq!(decision.reason, PolicyReason::SessionOnly);
        assert!(decision.session_only);
    }

    #[test]
    fn tool_output_requires_validation_before_confirmation() {
        let source = raw("preference", "workspace", "tool_output", 1.0);
        let (candidate, subject) = candidate(&source);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Pending);
        assert_eq!(decision.validation_status, ValidationStatus::Pending);
    }

    #[test]
    fn high_risk_markers_force_pending_for_any_kind() {
        let mut source = raw("entity", "project", "user", 1.0);
        source.statement = "Медицинский диагноз клиента известен команде".to_owned();
        let (candidate, subject) = candidate(&source);
        assert_eq!(classify_risk(&candidate), RiskClass::High);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.reason, PolicyReason::HighRiskRequiresApproval);
    }

    #[test]
    fn ambiguous_subject_falls_back_to_pending() {
        let mut source = raw("preference", "workspace", "user", 1.0);
        source.canonical_subject = "42".to_owned();
        let (candidate, subject) = candidate(&source);
        assert!(subject.ambiguous);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.reason, PolicyReason::AmbiguousSubject);
    }

    #[test]
    fn parse_rejects_oversized_malformed_and_unknown_fields() {
        let policy = ExtractionPolicy::default();
        let oversized = "x".repeat(policy.max_output_bytes + 1);
        assert!(matches!(
            parse_extraction(&oversized, &policy),
            Err(ExtractionError::OversizedOutput { .. })
        ));
        assert!(matches!(
            parse_extraction("not json", &policy),
            Err(ExtractionError::MalformedOutput {
                reason: MalformedReason::NotJson
            })
        ));
        assert!(matches!(
            parse_extraction("[]", &policy),
            Err(ExtractionError::MalformedOutput {
                reason: MalformedReason::NotAnObject
            })
        ));
        let unknown = serde_json::json!({
            "candidates": [],
            "extra": 1
        })
        .to_string();
        assert!(matches!(
            parse_extraction(&unknown, &policy),
            Err(ExtractionError::MalformedOutput {
                reason: MalformedReason::UnknownField
            })
        ));
    }

    #[test]
    fn parse_bounds_candidate_count_per_turn() {
        let policy = ExtractionPolicy::default();
        let candidates = (0..policy.max_candidates_per_turn + 1)
            .map(|_| serde_json::to_value(raw("preference", "workspace", "user", 0.9)).unwrap())
            .collect::<Vec<_>>();
        let payload = serde_json::json!({ "candidates": candidates }).to_string();
        assert!(matches!(
            parse_extraction(&payload, &policy),
            Err(ExtractionError::TooManyCandidates { .. })
        ));
    }

    #[test]
    fn unknown_enum_values_are_rejected() {
        let policy = ExtractionPolicy::default();
        let aliases = AliasTable::new();
        let mut source = raw("preference", "workspace", "user", 0.9);
        source.kind = "belief".to_owned();
        assert_eq!(
            validate_candidate(&source, &aliases, &policy),
            Err(ExtractionError::UnknownEnum { field: "kind" })
        );
        let mut source = raw("preference", "workspace", "user", 0.9);
        source.privacy = "top_secret".to_owned();
        assert_eq!(
            validate_candidate(&source, &aliases, &policy),
            Err(ExtractionError::UnknownEnum { field: "privacy" })
        );
    }

    #[test]
    fn evidence_locator_is_required() {
        let mut source = raw("preference", "workspace", "user", 0.9);
        source.evidence_locator = RawEvidenceLocator::default();
        assert_eq!(
            validate_candidate(&source, &AliasTable::new(), &ExtractionPolicy::default()),
            Err(ExtractionError::EmptyField("evidence_locator"))
        );
    }

    #[test]
    fn canonicalization_is_unicode_and_alias_aware() {
        assert_eq!(
            normalize_subject("  Язык   ИНТЕРФЕЙСА! ").unwrap(),
            "язык интерфейса"
        );
        // NFKC складывает полноширинные формы к обычным.
        assert_eq!(normalize_subject("ＵＩ").unwrap(), "ui");
        let mut aliases = AliasTable::new();
        aliases.register("UI язык", "entity:ui-language").unwrap();
        let resolved = canonicalize_subject("ui  ЯЗЫК", &aliases).unwrap();
        assert!(resolved.resolved_via_alias);
        assert_eq!(resolved.value, "entity ui language");
        assert!(!resolved.ambiguous);
    }

    #[test]
    fn triggers_are_detected_in_both_languages() {
        assert!(detect_explicit_trigger("Запомни: сборка идёт через cargo").is_some());
        assert!(detect_explicit_trigger("Please remember the build order").is_some());
        assert!(detect_explicit_trigger("расскажи про сборку").is_none());
    }

    #[test]
    fn scope_precedence_orders_task_over_session() {
        let mut records = vec![
            ActiveMemorySummary {
                id: "s".into(),
                kind: MemoryKind::Entity,
                canonical_subject: "x".into(),
                scope: MemoryScopeLevel::Session,
                statement: "a".into(),
                state: ConfirmationState::Confirmed,
            },
            ActiveMemorySummary {
                id: "t".into(),
                kind: MemoryKind::Entity,
                canonical_subject: "x".into(),
                scope: MemoryScopeLevel::Task,
                statement: "b".into(),
                state: ConfirmationState::Confirmed,
            },
        ];
        sort_by_scope_precedence(&mut records);
        assert_eq!(records[0].id, "t");
    }

    #[test]
    fn conflicts_need_same_key_and_incompatible_statement() {
        let (candidate, _) = candidate(&raw("preference", "workspace", "user", 0.9));
        let same_scope = ActiveMemorySummary {
            id: "m-1".into(),
            kind: MemoryKind::Preference,
            canonical_subject: candidate.canonical_subject.clone(),
            scope: MemoryScopeLevel::Workspace,
            statement: "Использовать английский язык в UI".into(),
            state: ConfirmationState::Confirmed,
        };
        assert_eq!(
            detect_conflict(&candidate, std::slice::from_ref(&same_scope)),
            ConflictVerdict::Conflict {
                existing_id: "m-1".into()
            }
        );
        let duplicate = ActiveMemorySummary {
            statement: candidate.statement.clone(),
            ..same_scope.clone()
        };
        assert_eq!(
            detect_conflict(&candidate, &[duplicate]),
            ConflictVerdict::Duplicate {
                existing_id: "m-1".into()
            }
        );
        let narrower = ActiveMemorySummary {
            scope: MemoryScopeLevel::Task,
            ..same_scope.clone()
        };
        assert_eq!(
            detect_conflict(&candidate, &[narrower]),
            ConflictVerdict::None
        );
        let pending = ActiveMemorySummary {
            state: ConfirmationState::PendingConfirmation,
            ..same_scope
        };
        assert_eq!(
            detect_conflict(&candidate, &[pending]),
            ConflictVerdict::None
        );
    }

    #[test]
    fn verification_only_source_of_confidence() {
        let policy = ExtractionPolicy::default();
        let valid = apply_verification(
            &VerificationOutcome {
                valid: Some(true),
                confidence: 0.9,
                checked_at_ms: 10,
                validator_version: VALIDATOR_VERSION.into(),
                evidence_digest: "digest-1".into(),
                reason: "file hash matches".into(),
            },
            &policy,
        );
        assert_eq!(valid.status, ValidationStatus::Valid);
        assert_eq!(valid.verification_confidence, 0.9);

        let weak = apply_verification(
            &VerificationOutcome {
                valid: Some(true),
                confidence: 0.5,
                checked_at_ms: 10,
                validator_version: VALIDATOR_VERSION.into(),
                evidence_digest: "digest-1".into(),
                reason: "weak".into(),
            },
            &policy,
        );
        assert_eq!(weak.status, ValidationStatus::Unknown);
        assert_eq!(weak.verification_confidence, 0.0);

        let invalid = apply_verification(
            &VerificationOutcome {
                valid: Some(false),
                confidence: 1.0,
                checked_at_ms: 10,
                validator_version: VALIDATOR_VERSION.into(),
                evidence_digest: "digest-1".into(),
                reason: "file changed".into(),
            },
            &policy,
        );
        assert_eq!(invalid.status, ValidationStatus::Invalid);
        assert!(!invalid.status.allows_retrieval());

        let unknown = apply_verification(
            &VerificationOutcome {
                valid: None,
                confidence: 1.0,
                checked_at_ms: 10,
                validator_version: VALIDATOR_VERSION.into(),
                evidence_digest: "digest-1".into(),
                reason: "timeout".into(),
            },
            &policy,
        );
        assert_eq!(unknown.status, ValidationStatus::Unknown);
    }

    #[test]
    fn file_evidence_distinguishes_changed_from_unreadable() {
        let policy = ExtractionPolicy::default();
        let matched = file_evidence_outcome("abc", Some("abc"), 5);
        assert_eq!(
            apply_verification(&matched, &policy).status,
            ValidationStatus::Valid
        );

        let changed = file_evidence_outcome("abc", Some("def"), 5);
        assert_eq!(
            apply_verification(&changed, &policy).status,
            ValidationStatus::Invalid
        );

        // Недоступная evidence и таймаут — это `unknown`: запись остаётся
        // pending, а не отвергается как ложная.
        let unreadable = file_evidence_outcome("abc", None, 5);
        assert_eq!(
            apply_verification(&unreadable, &policy).status,
            ValidationStatus::Unknown
        );

        let no_expectation = file_evidence_outcome("", Some("abc"), 5);
        assert_eq!(
            apply_verification(&no_expectation, &policy).status,
            ValidationStatus::Unknown
        );
    }

    #[test]
    fn validation_target_follows_source_trust_and_evidence() {
        let (user_candidate, _) = candidate(&raw("preference", "workspace", "user", 0.9));
        assert_eq!(validation_target(&user_candidate), None);

        let (tool_candidate, _) = candidate(&raw("entity", "project", "tool_output", 0.9));
        assert_eq!(
            validation_target(&tool_candidate),
            Some(ValidationTarget::Tool)
        );
        assert_eq!(ValidationTarget::Tool.timeout_ms(), 5_000);

        let mut source = raw("entity", "project", "document", 0.9);
        source.evidence_locator.file_path = "docs/plan.md".to_owned();
        let (document_candidate, _) = candidate(&source);
        assert_eq!(
            validation_target(&document_candidate),
            Some(ValidationTarget::Filesystem)
        );
        assert_eq!(ValidationTarget::Filesystem.timeout_ms(), 2_000);
    }

    #[test]
    fn hash_or_validator_change_invalidates_previous_check() {
        assert!(verification_is_stale("a", "v1", "b", "v1"));
        assert!(verification_is_stale("a", "v1", "a", "v2"));
        assert!(!verification_is_stale("a", "v1", "a", "v1"));
    }

    #[test]
    fn guard_enforces_turn_hour_and_breaker_limits() {
        let policy = ExtractionPolicy::default();
        let mut guard = ExtractionGuard::new();
        guard.begin_turn();
        for index in 0..policy.max_candidates_per_turn {
            guard
                .register_candidate(1_000 + index as u64, &policy)
                .expect("within turn limit");
        }
        assert!(matches!(
            guard.register_candidate(2_000, &policy),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::TurnLimit
            })
        ));

        // Часовой лимит держится поверх per-turn счётчика.
        let mut hourly = ExtractionGuard::new();
        for index in 0..policy.max_candidates_per_hour {
            hourly.begin_turn();
            hourly
                .register_candidate(1_000 + index as u64, &policy)
                .expect("within hourly limit");
        }
        hourly.begin_turn();
        assert!(matches!(
            hourly.register_candidate(2_000, &policy),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::HourlyLimit
            })
        ));

        let mut breaker = ExtractionGuard::new();
        breaker.register_malformed(0);
        breaker.register_malformed(1_000);
        assert!(!breaker.breaker_is_open(1_000));
        breaker.register_malformed(2_000);
        assert!(breaker.breaker_is_open(2_000));
        assert!(!breaker.breaker_is_open(2_000 + MALFORMED_BREAKER_COOLDOWN_MS));
        assert!(matches!(
            breaker.check_can_extract(
                ExtractionMode::Strict,
                Some(&TriggerMatch {
                    keyword: "запомни"
                }),
                2_500,
                &policy
            ),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::CircuitOpen
            })
        ));
    }

    #[test]
    fn malformed_outside_window_does_not_open_breaker() {
        let mut guard = ExtractionGuard::new();
        guard.register_malformed(0);
        guard.register_malformed(1_000);
        guard.register_malformed(MALFORMED_BREAKER_WINDOW_MS + 2_000);
        assert!(!guard.breaker_is_open(MALFORMED_BREAKER_WINDOW_MS + 2_000));
    }

    #[test]
    fn token_budget_stops_extraction_for_the_hour() {
        let policy = ExtractionPolicy::default();
        let mut guard = ExtractionGuard::new();
        guard.register_tokens(1_000, policy.max_tokens_per_hour);
        assert!(matches!(
            guard.check_can_extract(
                ExtractionMode::Strict,
                Some(&TriggerMatch {
                    keyword: "запомни"
                }),
                2_000,
                &policy
            ),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::TokenBudget
            })
        ));
        // Через час бюджет освобождается.
        assert!(guard
            .check_can_extract(
                ExtractionMode::Strict,
                Some(&TriggerMatch {
                    keyword: "запомни"
                }),
                1_000 + 60 * 60 * 1_000 + 1,
                &policy
            )
            .is_ok());
    }

    #[test]
    fn disabled_mode_still_allows_manual_trigger() {
        let policy = ExtractionPolicy::default();
        let mut guard = ExtractionGuard::new();
        assert!(matches!(
            guard.check_can_extract(ExtractionMode::Disabled, None, 1, &policy),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::ModeDisabled
            })
        ));
        assert!(guard
            .check_can_extract(
                ExtractionMode::Disabled,
                Some(&TriggerMatch {
                    keyword: "запомни"
                }),
                1,
                &policy
            )
            .is_ok());
    }

    #[test]
    fn retry_delays_are_bounded_to_two_attempts() {
        assert_eq!(ExtractionGuard::retry_delay_ms(0), Some(250));
        assert_eq!(ExtractionGuard::retry_delay_ms(1), Some(1_000));
        assert_eq!(ExtractionGuard::retry_delay_ms(2), None);
    }

    #[test]
    fn retrieval_excludes_expired_invalid_and_session_records() {
        assert!(is_retrievable(
            ConfirmationState::Confirmed,
            ValidationStatus::Valid,
            MemoryKind::Entity,
            Some(100),
            50
        ));
        assert!(!is_retrievable(
            ConfirmationState::Confirmed,
            ValidationStatus::Valid,
            MemoryKind::Entity,
            Some(100),
            100
        ));
        assert!(!is_retrievable(
            ConfirmationState::Confirmed,
            ValidationStatus::Invalid,
            MemoryKind::Entity,
            None,
            0
        ));
        assert!(!is_retrievable(
            ConfirmationState::PendingConfirmation,
            ValidationStatus::Valid,
            MemoryKind::Entity,
            None,
            0
        ));
        assert!(!is_retrievable(
            ConfirmationState::Confirmed,
            ValidationStatus::Valid,
            MemoryKind::SessionSummary,
            None,
            0
        ));
    }

    #[test]
    fn policy_path_costs_far_less_than_the_latency_budget() {
        // The plan budgets <= 200 ms p95 of added turn latency. Everything in
        // this module is deterministic and I/O-free; the only remaining cost
        // is the model call, which runs after the answer has been sent.
        let policy = ExtractionPolicy::default();
        let aliases = AliasTable::new();
        let payload = serde_json::json!({
            "candidates": (0..policy.max_candidates_per_turn)
                .map(|_| serde_json::to_value(raw("preference", "workspace", "user", 0.9)).unwrap())
                .collect::<Vec<_>>()
        })
        .to_string();
        let started = std::time::Instant::now();
        for _ in 0..100 {
            let parsed = parse_extraction(&payload, &policy).expect("parses");
            for candidate in &parsed {
                let (candidate, subject) =
                    validate_candidate(candidate, &aliases, &policy).expect("validates");
                let _ = evaluate(
                    &candidate,
                    &TurnContext::strict_with_trigger("запомни"),
                    &subject,
                    &policy,
                );
            }
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(200),
            "100 full policy passes took {elapsed:?}, budget is 200 ms for one turn"
        );
    }

    // ---------------------------------------------------------------------
    // Ambient (04.6)
    // ---------------------------------------------------------------------

    #[test]
    fn ambient_never_auto_confirms_in_any_combination() {
        let kinds = [
            MemoryKind::Preference,
            MemoryKind::Constraint,
            MemoryKind::Decision,
            MemoryKind::Entity,
            MemoryKind::Lesson,
            MemoryKind::SessionSummary,
        ];
        let scopes = [
            MemoryScopeLevel::Task,
            MemoryScopeLevel::Project,
            MemoryScopeLevel::Workspace,
            MemoryScopeLevel::Session,
        ];
        let privacies = [PrivacyLevel::Normal, PrivacyLevel::Sensitive];
        let confidences = [0.0, 0.5, 0.85, 0.95, 1.0];
        let subjects = [
            CanonicalSubject {
                value: "язык интерфейса".to_owned(),
                resolved_via_alias: true,
                ambiguous: false,
            },
            CanonicalSubject {
                value: "x".to_owned(),
                resolved_via_alias: false,
                ambiguous: true,
            },
        ];
        let modes = [
            ExtractionMode::Strict,
            ExtractionMode::Open,
            ExtractionMode::Disabled,
        ];
        let policy = ExtractionPolicy::default();
        let mut checked = 0usize;
        for kind in kinds {
            for scope in scopes {
                for privacy in privacies {
                    for confidence in confidences {
                        for subject in &subjects {
                            for mode in modes {
                                let candidate = Candidate {
                                    kind,
                                    statement: "использовать русский язык в интерфейсе".to_owned(),
                                    scope,
                                    canonical_subject: subject.value.clone(),
                                    raw_subject: subject.value.clone(),
                                    model_confidence: confidence,
                                    verification_confidence: 0.0,
                                    reason: "услышано".to_owned(),
                                    evidence: RawEvidenceLocator {
                                        episode_id: "episode-1".to_owned(),
                                        ..RawEvidenceLocator::default()
                                    },
                                    privacy,
                                    source_trust: SourceTrust::Ambient,
                                    suggested_ttl_ms: 0,
                                };
                                let decision = evaluate(
                                    &candidate,
                                    &TurnContext::ambient(mode),
                                    subject,
                                    &policy,
                                );
                                assert_ne!(
                                    decision.outcome,
                                    PolicyOutcome::AutoConfirm,
                                    "ambient auto-confirmed {kind:?}/{scope:?}/{privacy:?}"
                                );
                                assert_ne!(decision.state, ConfirmationState::Confirmed);
                                checked += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(checked, 6 * 4 * 2 * 5 * 2 * 3);
    }

    #[test]
    fn ambient_candidate_stays_pending_even_with_a_user_trigger() {
        // Явный триггер в диалоге не делает услышанное утверждением
        // пользователя: источник остаётся ambient.
        let candidate = Candidate {
            kind: MemoryKind::Preference,
            statement: "использовать тёмную тему".to_owned(),
            scope: MemoryScopeLevel::Workspace,
            canonical_subject: "тема оформления".to_owned(),
            raw_subject: "тема оформления".to_owned(),
            model_confidence: 1.0,
            verification_confidence: 0.0,
            reason: "услышано".to_owned(),
            evidence: RawEvidenceLocator {
                episode_id: "episode-1".to_owned(),
                ..RawEvidenceLocator::default()
            },
            privacy: PrivacyLevel::Normal,
            source_trust: SourceTrust::Ambient,
            suggested_ttl_ms: 0,
        };
        let subject = CanonicalSubject {
            value: "тема оформления".to_owned(),
            resolved_via_alias: true,
            ambiguous: false,
        };
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Pending);
        assert_eq!(decision.reason, PolicyReason::AmbientNeverAutoConfirms);
    }

    #[test]
    fn ambient_secret_is_still_rejected_before_the_ambient_gate() {
        let candidate = Candidate {
            kind: MemoryKind::Entity,
            statement: "ключ sk-abcdef".to_owned(),
            scope: MemoryScopeLevel::Workspace,
            canonical_subject: "ключ".to_owned(),
            raw_subject: "ключ".to_owned(),
            model_confidence: 0.9,
            verification_confidence: 0.0,
            reason: "услышано".to_owned(),
            evidence: RawEvidenceLocator {
                episode_id: "episode-1".to_owned(),
                ..RawEvidenceLocator::default()
            },
            privacy: PrivacyLevel::Normal,
            source_trust: SourceTrust::Ambient,
            suggested_ttl_ms: 0,
        };
        let subject = CanonicalSubject {
            value: "ключ".to_owned(),
            resolved_via_alias: false,
            ambiguous: false,
        };
        let decision = evaluate(
            &candidate,
            &TurnContext::ambient(ExtractionMode::Strict),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Reject);
        assert_eq!(decision.reason, PolicyReason::SecretNeverStored);
    }

    #[test]
    fn ambient_mode_defaults_to_pending_and_fails_safe_on_garbage() {
        assert_eq!(AmbientMemoryMode::parse(None), AmbientMemoryMode::Pending);
        assert_eq!(
            AmbientMemoryMode::parse(Some(" PENDING ")),
            AmbientMemoryMode::Pending
        );
        assert_eq!(
            AmbientMemoryMode::parse(Some("off")),
            AmbientMemoryMode::Off
        );
        // Аналога `open` у ambient нет: он не «почти pending», а мусор.
        assert_eq!(
            AmbientMemoryMode::parse(Some("open")),
            AmbientMemoryMode::Off
        );
        assert_eq!(AmbientMemoryMode::parse(Some("да")), AmbientMemoryMode::Off);
    }

    #[test]
    fn ambient_runs_in_strict_mode_although_the_dialog_gate_would_refuse_it() {
        let policy = ExtractionPolicy::default();
        let mut guard = ExtractionGuard::new();
        // Диалоговый гейт отвергает всё без триггера в strict-режиме...
        assert_eq!(
            guard.check_can_extract(ExtractionMode::Strict, None, 0, &policy),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::NoExplicitTrigger
            })
        );
        // ...а ambient-гейт триггера не ждёт: им служит закрытие эпизода.
        assert!(guard
            .check_can_extract_ambient(
                AmbientMemoryMode::Pending,
                ExtractionMode::Strict,
                0,
                &policy
            )
            .is_ok());
    }

    #[test]
    fn general_switch_outranks_the_ambient_one() {
        let policy = ExtractionPolicy::default();
        let mut guard = ExtractionGuard::new();
        assert_eq!(
            guard.check_can_extract_ambient(
                AmbientMemoryMode::Pending,
                ExtractionMode::Disabled,
                0,
                &policy
            ),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::ModeDisabled
            })
        );
        assert_eq!(
            guard.check_can_extract_ambient(
                AmbientMemoryMode::Off,
                ExtractionMode::Strict,
                0,
                &policy
            ),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::ModeDisabled
            })
        );
    }

    #[test]
    fn ambient_budgets_are_counted_apart_from_the_dialog_ones() {
        let policy = ExtractionPolicy::default();
        let mut guard = ExtractionGuard::new();
        for _ in 0..policy.max_ambient_candidates_per_hour {
            assert!(guard.register_ambient_candidate(1_000, &policy).is_ok());
        }
        assert_eq!(
            guard.register_ambient_candidate(1_000, &policy),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::AmbientCandidateLimit
            })
        );
        for _ in 0..policy.max_ambient_episodes_per_hour {
            assert!(guard.register_ambient_episode(1_000, &policy).is_ok());
        }
        assert_eq!(
            guard.register_ambient_episode(1_000, &policy),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::AmbientEpisodeLimit
            })
        );
        // Диалоговый путь этих трат не заметил: у него свой счётчик и своя
        // причина отказа.
        guard.begin_turn();
        assert!(guard.register_candidate(1_000, &policy).is_ok());
        // Свой бюджет токенов: ambient не тратит часовой бюджет диалога.
        guard.register_ambient_tokens(1_000, policy.max_ambient_tokens_per_hour);
        assert_eq!(
            guard.check_can_extract_ambient(
                AmbientMemoryMode::Pending,
                ExtractionMode::Strict,
                1_000,
                &policy
            ),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::TokenBudget
            })
        );
        assert!(guard
            .check_can_extract(
                ExtractionMode::Strict,
                Some(&TriggerMatch {
                    keyword: "запомни"
                }),
                1_000,
                &policy
            )
            .is_ok());
    }

    #[test]
    fn constraint_and_decision_never_come_from_ambient() {
        assert!(!ambient_kind_allowed(MemoryKind::Constraint));
        assert!(!ambient_kind_allowed(MemoryKind::Decision));
        // session_summary тоже не принимается: у речи нет диалоговой сессии,
        // а session-note не проходит через очередь подтверждения.
        assert!(!ambient_kind_allowed(MemoryKind::SessionSummary));
        assert!(ambient_kind_allowed(MemoryKind::Preference));
        assert!(ambient_kind_allowed(MemoryKind::Entity));
        assert!(ambient_kind_allowed(MemoryKind::Lesson));
    }

    #[test]
    fn a_third_person_statement_is_raised_to_sensitive_and_stays_pending() {
        let mut candidate = Candidate {
            kind: MemoryKind::Entity,
            statement: "она просила присылать отчёты по понедельникам".to_owned(),
            scope: MemoryScopeLevel::Workspace,
            canonical_subject: "отчёты".to_owned(),
            raw_subject: "отчёты".to_owned(),
            model_confidence: 1.0,
            verification_confidence: 0.0,
            reason: "услышано".to_owned(),
            evidence: RawEvidenceLocator {
                episode_id: "episode-1".to_owned(),
                ..RawEvidenceLocator::default()
            },
            privacy: PrivacyLevel::Normal,
            source_trust: SourceTrust::Ambient,
            suggested_ttl_ms: 0,
        };
        assert!(apply_ambient_privacy_floor(&mut candidate));
        assert_eq!(candidate.privacy, PrivacyLevel::Sensitive);
        assert!(candidate.privacy.redacts_body_by_default());
        let subject = CanonicalSubject {
            value: "отчёты".to_owned(),
            resolved_via_alias: true,
            ambiguous: false,
        };
        let decision = evaluate(
            &candidate,
            &TurnContext::ambient(ExtractionMode::Strict),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Pending);
        assert_eq!(decision.risk, RiskClass::High);

        // Утверждение от первого лица класс не поднимает.
        let mut plain = candidate.clone();
        plain.statement = "предпочитаю тёмную тему".to_owned();
        plain.privacy = PrivacyLevel::Normal;
        assert!(!apply_ambient_privacy_floor(&mut plain));
        assert_eq!(plain.privacy, PrivacyLevel::Normal);
    }

    #[test]
    fn third_party_detection_covers_pronouns_and_names() {
        assert!(mentions_third_party("он сказал, что сборка сломана"));
        assert!(mentions_third_party("Это просил передать Андрей"));
        assert!(!mentions_third_party("Хочу отчёты по понедельникам"));
    }

    #[test]
    fn ambient_locator_carries_the_episode_and_reaches_provenance() {
        let mut raw = raw("preference", "workspace", "ambient", 0.9);
        raw.evidence_locator = RawEvidenceLocator {
            episode_id: "episode-42".to_owned(),
            ..RawEvidenceLocator::default()
        };
        // Локатор с одним эпизодом — не пустой: иначе ambient-кандидат не
        // прошёл бы валидацию вовсе.
        assert!(!raw.evidence_locator.is_empty());
        let (candidate, _) = candidate(&raw);
        assert_eq!(candidate.source_trust, SourceTrust::Ambient);
        assert!(candidate.source_trust.requires_validation());
        assert!(!candidate.source_trust.can_ground_strict_save());
        assert_eq!(candidate.evidence.episode_id, "episode-42");
        // Хеш текста для ambient пуст: по правилу 04.1 хеш короткой фразы
        // приравнивается к её содержимому.
        assert!(candidate.evidence.content_hash.is_empty());
        let json = candidate
            .evidence
            .to_provenance_json()
            .expect("locator serialises");
        assert!(json.contains("episode-42"));
    }

    #[test]
    fn ttl_is_bounded_by_kind_default() {
        let mut source = raw("preference", "workspace", "user", 0.9);
        source.suggested_ttl_ms = 10 * 365 * DAY_MS;
        let (capped, _) = candidate(&source);
        assert_eq!(bounded_ttl(&capped), 180 * DAY_MS);
        source.suggested_ttl_ms = DAY_MS;
        let (shorter, _) = candidate(&source);
        assert_eq!(bounded_ttl(&shorter), DAY_MS);
    }

