//! Queen-owned safety ceiling for dynamic sub-agent fan-out.

pub use evohime_hive_contract::settings::*;
pub use evohime_model_gateway::ProviderLimits;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_ceilings_and_validate() {
        let settings = HiveSettings::default();
        assert_eq!(settings.max_subagents_per_worker, 16);
        assert_eq!(settings.max_parallel_subagents, 4);
        assert_eq!(settings.max_total_subagents_per_workflow, 64);
        assert_eq!(settings.max_delegation_depth, DEFAULT_MAX_DELEGATION_DEPTH);
        assert!(settings.validate().is_ok());
        assert_eq!(settings.effective_worker_ceiling(), 16);
        assert_eq!(settings.effective_parallel_limit(), 4);
    }

    #[test]
    fn zero_semantics_are_explicit() {
        let mut settings = HiveSettings {
            max_subagents_per_worker: 0,
            max_parallel_subagents: 0,
            ..HiveSettings::default()
        };
        assert!(settings.validate().is_ok());
        assert_eq!(settings.effective_worker_ceiling(), 64);
        assert_eq!(settings.effective_parallel_limit(), 64);

        settings.max_total_subagents_per_workflow = 0;
        assert_eq!(
            settings.validate(),
            Err(HiveSettingsError::MissingWorkflowGuard)
        );
    }

    #[test]
    fn hard_limits_reject_hostile_values() {
        let settings = HiveSettings {
            max_subagents_per_worker: MAX_SUBAGENT_HARD_LIMIT + 1,
            ..HiveSettings::default()
        };
        assert!(settings.validate().is_err());
        let settings = HiveSettings {
            max_parallel_subagents: MAX_PARALLEL_HARD_LIMIT + 1,
            ..HiveSettings::default()
        };
        assert!(settings.validate().is_err());
        let settings = HiveSettings {
            max_total_subagents_per_workflow: MAX_SUBAGENT_HARD_LIMIT + 1,
            ..HiveSettings::default()
        };
        assert!(settings.validate().is_err());
        let zero_depth = HiveSettings {
            max_delegation_depth: 0,
            ..HiveSettings::default()
        };
        assert!(matches!(
            zero_depth.validate(),
            Err(HiveSettingsError::InvalidDelegationDepth { .. })
        ));
        let over_depth = HiveSettings {
            max_delegation_depth: MAX_DELEGATION_DEPTH_HARD_LIMIT + 1,
            ..HiveSettings::default()
        };
        assert!(matches!(
            over_depth.validate(),
            Err(HiveSettingsError::InvalidDelegationDepth { .. })
        ));
    }

    #[test]
    fn heuristic_grows_with_task_complexity_but_stays_a_ceiling() {
        let settings = HiveSettings {
            max_subagents_per_worker: 8,
            ..HiveSettings::default()
        };
        assert_eq!(
            settings.choose_delegation_ceiling(DelegationComplexity::default()),
            1
        );
        assert_eq!(
            settings.choose_delegation_ceiling(DelegationComplexity {
                goal_chars: 0,
                context_items: 0,
                grant_count: 0,
                independent_areas: 4,
            }),
            3
        );
        assert!(
            settings.choose_delegation_ceiling(DelegationComplexity {
                goal_chars: 100_000,
                context_items: 1_000,
                grant_count: 4,
                independent_areas: 100,
            }) <= 8
        );
    }

    #[test]
    fn observation_budget_and_intervals_validate_and_keep_compatible_defaults() {
        let settings = HiveSettings::default();
        assert_eq!(
            settings.local_observation_interval_secs,
            DEFAULT_LOCAL_OBSERVATION_INTERVAL_SECS
        );
        assert_eq!(
            settings.llm_dispatch_interval_secs,
            DEFAULT_LLM_DISPATCH_INTERVAL_SECS
        );
        assert_eq!(settings.daily_budget_usd, DEFAULT_DAILY_BUDGET_USD);

        let legacy = br#"{"schema_version":1,"max_subagents_per_worker":1,"max_parallel_subagents":1,"max_total_subagents_per_workflow":2,"pulse_interval_secs":900}"#;
        let loaded = HiveSettings::from_json(legacy).expect("legacy settings load");
        assert_eq!(
            loaded.local_observation_interval_secs,
            DEFAULT_LOCAL_OBSERVATION_INTERVAL_SECS
        );
        assert_eq!(loaded.llm_dispatch_interval_secs, 900);
        assert_eq!(loaded.daily_budget_usd, DEFAULT_DAILY_BUDGET_USD);
        assert_eq!(loaded.max_delegation_depth, DEFAULT_MAX_DELEGATION_DEPTH);

        let zero_budget = HiveSettings {
            daily_budget_usd: 0.0,
            ..HiveSettings::default()
        };
        assert!(zero_budget.validate().is_ok());

        let too_fast_local = HiveSettings {
            local_observation_interval_secs: MIN_LOCAL_OBSERVATION_INTERVAL_SECS - 1,
            ..HiveSettings::default()
        };
        assert!(matches!(
            too_fast_local.validate(),
            Err(HiveSettingsError::InvalidObservationInterval {
                field: "local_observation_interval_secs",
                ..
            })
        ));

        let too_fast_llm = HiveSettings {
            llm_dispatch_interval_secs: MIN_LLM_DISPATCH_INTERVAL_SECS - 1,
            ..HiveSettings::default()
        };
        assert!(matches!(
            too_fast_llm.validate(),
            Err(HiveSettingsError::InvalidObservationInterval {
                field: "llm_dispatch_interval_secs",
                ..
            })
        ));

        let negative_budget = HiveSettings {
            daily_budget_usd: -0.1,
            ..HiveSettings::default()
        };
        assert!(matches!(
            negative_budget.validate(),
            Err(HiveSettingsError::InvalidDailyBudget { .. })
        ));
    }

    #[test]
    fn json_roundtrip_revalidates_on_load() {
        let settings = HiveSettings {
            max_subagents_per_worker: 3,
            max_parallel_subagents: 2,
            max_total_subagents_per_workflow: 9,
            schema_version: HIVE_SETTINGS_SCHEMA_VERSION,
            ..HiveSettings::default()
        };
        let json = settings.to_json().expect("serialize");
        assert_eq!(HiveSettings::from_json(&json).expect("load"), settings);

        let invalid = br#"{"schema_version":1,"max_subagents_per_worker":1,"max_parallel_subagents":1,"max_total_subagents_per_workflow":0}"#;
        assert!(matches!(
            HiveSettings::from_json(invalid),
            Err(HiveSettingsError::MissingWorkflowGuard)
        ));
    }

    #[test]
    fn eva_workspace_path_validation() {
        let missing = HiveSettings {
            watched_repositories: vec!["C:\\definitely\\missing\\eva-workspace".into()],
            ..HiveSettings::default()
        };
        assert!(matches!(
            missing.validate(),
            Err(HiveSettingsError::InvalidEvaWorkspace(_))
        ));
        let plain = tempfile::tempdir().expect("directory");
        let not_git = HiveSettings {
            watched_repositories: vec![plain.path().to_string_lossy().into_owned()],
            ..HiveSettings::default()
        };
        assert!(matches!(
            not_git.validate(),
            Err(HiveSettingsError::InvalidEvaWorkspace(_))
        ));
        assert!(HiveSettings::default().validate().is_ok());
    }

    #[test]
    fn migrates_legacy_eva_workspace_path_and_separates_it_from_watched_repositories() {
        let git_repo = tempfile::tempdir().expect("git directory");
        let status = std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(git_repo.path())
            .status()
            .expect("run git init");
        assert!(status.success());

        let repo_path = git_repo.path().to_string_lossy().into_owned();
        let legacy_json = serde_json::json!({
            "schema_version": HIVE_SETTINGS_SCHEMA_VERSION,
            "max_subagents_per_worker": 16,
            "max_parallel_subagents": 4,
            "max_total_subagents_per_workflow": 64,
            "eva_workspace_path": repo_path,
            "watched_repositories": [repo_path],
            "proactive_evolution_enabled": true
        })
        .to_string()
        .into_bytes();

        let loaded = HiveSettings::from_json(&legacy_json).expect("legacy settings migrate");
        assert_eq!(loaded.eva_workspace_path(), Some(repo_path.as_str()));
        assert!(loaded.watched_repositories.is_empty());
        assert!(loaded.proactive_evolution_enabled);

        // Also migrates records written when `eva_workspace_path` key was absent
        // and stored at `watched_repositories[0]`.
        let conflated_json = serde_json::json!({
            "schema_version": HIVE_SETTINGS_SCHEMA_VERSION,
            "max_subagents_per_worker": 16,
            "max_parallel_subagents": 4,
            "max_total_subagents_per_workflow": 64,
            "watched_repositories": [repo_path],
            "proactive_evolution_enabled": true
        })
        .to_string()
        .into_bytes();
        let migrated =
            HiveSettings::from_json(&conflated_json).expect("conflated settings migrate");
        assert_eq!(migrated.eva_workspace_path(), Some(repo_path.as_str()));
        assert!(migrated.watched_repositories.is_empty());
    }

    #[test]
    fn core_validates_eva_workspace_path() {
        assert!(matches!(
            validate_eva_workspace_path(""),
            Err(HiveSettingsError::InvalidEvaWorkspace(_))
        ));
        assert!(matches!(
            validate_eva_workspace_path("."),
            Err(HiveSettingsError::InvalidEvaWorkspace(_))
        ));
        assert!(matches!(
            validate_eva_workspace_path("relative/eva-workspace"),
            Err(HiveSettingsError::InvalidEvaWorkspace(_))
        ));
        assert!(matches!(
            validate_eva_workspace_path("C:\\definitely\\missing\\eva-workspace"),
            Err(HiveSettingsError::InvalidEvaWorkspace(_))
        ));

        let plain = tempfile::tempdir().expect("directory");
        assert!(matches!(
            validate_eva_workspace_path(&plain.path().to_string_lossy()),
            Err(HiveSettingsError::InvalidEvaWorkspace(_))
        ));

        let git_repo = tempfile::tempdir().expect("git directory");
        let status = std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(git_repo.path())
            .status()
            .expect("run git init");
        assert!(status.success());
        assert!(validate_eva_workspace_path(&git_repo.path().to_string_lossy()).is_ok());
    }

    #[test]
    fn workflow_capacity_counts_spawned_subagents() {
        let settings = HiveSettings {
            max_subagents_per_worker: 8,
            max_parallel_subagents: 4,
            max_total_subagents_per_workflow: 10,
            schema_version: HIVE_SETTINGS_SCHEMA_VERSION,
            ..HiveSettings::default()
        };
        assert_eq!(settings.ensure_workflow_capacity(8, 0), Ok(8));
        assert_eq!(settings.ensure_workflow_capacity(6, 4), Ok(6));
        assert_eq!(
            settings.ensure_workflow_capacity(8, 4),
            Err(HiveSettingsError::WorkflowCeilingAboveRemaining {
                requested: 8,
                remaining: 6
            })
        );
        assert_eq!(
            settings.ensure_workflow_capacity(9, 0),
            Err(HiveSettingsError::WorkerCeilingAboveLimit {
                requested: 9,
                limit: 8
            })
        );
    }

    #[test]
    fn provider_request_limits_accept_realistic_and_u32_max_values() {
        let settings = HiveSettings {
            hive_provider_limits: ProviderLimits {
                rpm: 7,
                rph: 200,
                rp4h: 600,
                rpd: 10_080,
            },
            hive_fallback_provider_limits: ProviderLimits {
                rpm: 10,
                rph: 600,
                rp4h: 2_400,
                rpd: 14_400,
            },
            ..HiveSettings::default()
        };
        assert!(settings.validate().is_ok());

        let max_limits = HiveSettings {
            hive_provider_limits: ProviderLimits {
                rpm: u32::MAX,
                rph: u32::MAX,
                rp4h: u32::MAX,
                rpd: u32::MAX,
            },
            ..HiveSettings::default()
        };
        assert!(max_limits.validate().is_ok());
    }
}
