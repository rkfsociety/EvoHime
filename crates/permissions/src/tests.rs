use super::*;
use futures_executor::block_on;

#[test]
fn default_policy_allows_read_and_asks_for_write() {
    block_on(async {
        let engine = PermissionEngine::new();
        assert_eq!(
            engine.check(Permission::FilesystemRead).await,
            PermissionDecision::Allowed
        );
        assert_eq!(
            engine.check(Permission::FilesystemWrite).await,
            PermissionDecision::NeedsApproval
        );
    });
}

#[test]
fn approval_is_one_shot() {
    block_on(async {
        let engine = PermissionEngine::new();
        let task_id = Uuid::new_v4();
        let request = engine
            .create_approval(
                task_id,
                "filesystem.write",
                Permission::FilesystemWrite,
                "a.txt",
            )
            .await;
        assert_eq!(request.task_id, task_id);
        assert_eq!(
            engine.resolve(request.id, true).await,
            Some(ApprovalState::Granted)
        );
        assert_eq!(engine.resolve(request.id, false).await, None);
    });
}

#[test]
fn approval_call_hash_is_canonical_and_scope_is_normalized() {
    block_on(async {
        let engine = PermissionEngine::new();
        let input = serde_json::from_str::<serde_json::Value>(
            r#"{"path":".\\src\\main.rs","content":"x","metadata":{"b":2,"a":1}}"#,
        )
        .unwrap();
        let equivalent = serde_json::from_str::<serde_json::Value>(
            r#"{"metadata":{"a":1,"b":2},"content":"x","path":".\\src\\main.rs"}"#,
        )
        .unwrap();
        let request = engine
            .create_approval_scoped_for_call(
                Uuid::new_v4(),
                None,
                "filesystem.write",
                Permission::FilesystemWrite,
                r#".\src\main.rs"#,
                &input,
            )
            .await;
        engine.resolve(request.id, true).await.expect("granted");

        assert_eq!(
            engine
                .approval_matches_call(
                    request.id,
                    CallIdentity {
                        task_id: request.task_id,
                        session_id: None,
                        tool_name: "filesystem.write",
                        permission: Permission::FilesystemWrite,
                        scope: "src/main.rs",
                        input: &equivalent,
                    },
                )
                .await,
            Some(ApprovalState::Granted)
        );
    });
}

#[test]
fn claimed_approval_is_removed_atomically() {
    block_on(async {
        let engine = PermissionEngine::new();
        let task_id = Uuid::new_v4();
        let input = serde_json::json!({"path": "a.txt", "content": "x"});
        let request = engine
            .create_approval_scoped_for_call(
                task_id,
                None,
                "filesystem.write",
                Permission::FilesystemWrite,
                "a.txt",
                &input,
            )
            .await;
        engine.resolve(request.id, true).await.expect("granted");

        assert_eq!(
            engine
                .claim_approval_for_call(
                    request.id,
                    CallIdentity {
                        task_id,
                        session_id: None,
                        tool_name: "filesystem.write",
                        permission: Permission::FilesystemWrite,
                        scope: "a.txt",
                        input: &input,
                    },
                )
                .await,
            Some(ApprovalState::Granted)
        );
        assert_eq!(
            engine
                .claim_approval_for_call(
                    request.id,
                    CallIdentity {
                        task_id,
                        session_id: None,
                        tool_name: "filesystem.write",
                        permission: Permission::FilesystemWrite,
                        scope: "a.txt",
                        input: &input,
                    },
                )
                .await,
            None
        );
    });
}

#[test]
fn session_override_beats_global_mode() {
    block_on(async {
        let engine = PermissionEngine::new();
        let session = Uuid::new_v4();
        engine
            .set_session_mode(session, Permission::FilesystemWrite, PermissionMode::Allow)
            .await;
        assert_eq!(
            engine
                .check_scoped(
                    Permission::FilesystemWrite,
                    &PermissionCheck {
                        session_id: Some(session),
                        path: None,
                        command: None,
                    },
                )
                .await,
            PermissionDecision::Allowed
        );
        assert_eq!(
            engine.check(Permission::FilesystemWrite).await,
            PermissionDecision::NeedsApproval
        );
    });
}

#[test]
fn path_grant_allows_matching_prefix() {
    block_on(async {
        let engine = PermissionEngine::new();
        let session = Uuid::new_v4();
        engine
            .set_path_grant(
                Permission::FilesystemWrite,
                "docs",
                PermissionMode::Allow,
                Some(session),
                None,
            )
            .await;
        assert_eq!(
            engine
                .check_scoped(
                    Permission::FilesystemWrite,
                    &PermissionCheck {
                        session_id: Some(session),
                        path: Some("docs/readme.md"),
                        command: None,
                    },
                )
                .await,
            PermissionDecision::Allowed
        );
        assert_eq!(
            engine
                .check_scoped(
                    Permission::FilesystemWrite,
                    &PermissionCheck {
                        session_id: Some(session),
                        path: Some("src/main.rs"),
                        command: None,
                    },
                )
                .await,
            PermissionDecision::NeedsApproval
        );
    });
}

#[test]
fn grant_remembers_path_for_session() {
    block_on(async {
        let engine = PermissionEngine::new();
        let session = Uuid::new_v4();
        let task = Uuid::new_v4();
        let request = engine
            .create_approval_scoped(
                task,
                Some(session),
                "filesystem.write",
                Permission::FilesystemWrite,
                "tmp/note.txt",
            )
            .await;
        engine
            .resolve_with_options(request.id, true, true)
            .await
            .expect("granted");

        assert_eq!(
            engine
                .check_scoped(
                    Permission::FilesystemWrite,
                    &PermissionCheck {
                        session_id: Some(session),
                        path: Some("tmp/note.txt"),
                        command: None,
                    },
                )
                .await,
            PermissionDecision::Allowed
        );

        let audit = engine.audit_log().await;
        assert!(audit
            .iter()
            .any(|entry| { entry.decision == ApprovalState::Granted && entry.remembered_path }));
        assert!(audit
            .iter()
            .any(|entry| entry.decision == ApprovalState::Pending));
    });
}

#[test]
fn audit_sender_receives_pending_and_resolved() {
    block_on(async {
        let engine = PermissionEngine::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        engine.attach_audit_sender(tx).await;

        let request = engine
            .create_approval(
                Uuid::new_v4(),
                "shell.execute",
                Permission::ShellExecute,
                "workspace",
            )
            .await;
        engine.resolve(request.id, false).await.expect("denied");

        let pending = rx.recv().await.expect("pending audit");
        assert_eq!(pending.decision, ApprovalState::Pending);
        assert_eq!(pending.approval_id, request.id);

        let denied = rx.recv().await.expect("denied audit");
        assert_eq!(denied.decision, ApprovalState::Denied);
        assert_eq!(denied.approval_id, request.id);
    });
}

#[test]
fn path_deny_overrides_session_allow() {
    block_on(async {
        let engine = PermissionEngine::new();
        let session = Uuid::new_v4();
        engine
            .set_session_mode(session, Permission::ShellExecute, PermissionMode::Allow)
            .await;
        engine
            .set_path_grant(
                Permission::ShellExecute,
                "secrets",
                PermissionMode::Deny,
                Some(session),
                None,
            )
            .await;
        assert_eq!(
            engine
                .check_scoped(
                    Permission::ShellExecute,
                    &PermissionCheck {
                        session_id: Some(session),
                        path: Some("secrets/key.env"),
                        command: None,
                    },
                )
                .await,
            PermissionDecision::Denied
        );
    });
}

#[test]
fn scopes_snapshot_roundtrip_preserves_grants() {
    block_on(async {
        let engine = PermissionEngine::new();
        let session = Uuid::new_v4();
        engine
            .set_session_mode(session, Permission::FilesystemWrite, PermissionMode::Allow)
            .await;
        engine
            .set_path_grant(
                Permission::FilesystemWrite,
                "src",
                PermissionMode::Allow,
                Some(session),
                Some(Duration::from_secs(3_600)),
            )
            .await;
        engine
            .set_path_grant(
                Permission::ShellExecute,
                "scripts",
                PermissionMode::Deny,
                None,
                None,
            )
            .await;

        let snapshot = engine.export_scopes().await;
        let restored = PermissionEngine::new();
        restored.import_scopes(snapshot).await;

        assert_eq!(
            restored
                .check_scoped(
                    Permission::FilesystemWrite,
                    &PermissionCheck {
                        session_id: Some(session),
                        path: Some("src/main.rs"),
                        command: None,
                    },
                )
                .await,
            PermissionDecision::Allowed
        );
        assert_eq!(
            restored
                .check_scoped(
                    Permission::ShellExecute,
                    &PermissionCheck {
                        session_id: None,
                        path: Some("scripts/run.sh"),
                        command: None,
                    },
                )
                .await,
            PermissionDecision::Denied
        );
        assert_eq!(restored.list_session_overrides().await.len(), 1);
        assert_eq!(restored.list_path_grants().await.len(), 2);
    });
}

#[test]
fn import_scopes_skips_expired_path_grants() {
    block_on(async {
        let session = Uuid::new_v4();
        let snapshot = PermissionScopesSnapshot {
            session_overrides: vec![],
            path_grants: vec![PathGrant {
                permission: Permission::FilesystemWrite,
                path: "tmp".into(),
                session_id: Some(session),
                mode: PermissionMode::Allow,
                expires_at_ms: Some(1), // far in the past
            }],
        };
        let engine = PermissionEngine::new();
        engine.import_scopes(snapshot).await;
        assert!(engine.list_path_grants().await.is_empty());
        assert_eq!(
            engine
                .check_scoped(
                    Permission::FilesystemWrite,
                    &PermissionCheck {
                        session_id: Some(session),
                        path: Some("tmp/a.txt"),
                        command: None,
                    },
                )
                .await,
            PermissionDecision::NeedsApproval
        );
    });
}

#[test]
fn hard_policy_deny_beats_runtime_grants_and_session_modes() {
    block_on(async {
        let session = Uuid::new_v4();
        let engine = PermissionEngine::new();
        engine
            .set_policy_rules(PolicyRuleSet::new(vec![PolicyRule {
                permission: Permission::ShellExecute,
                pattern: "rm *".into(),
                mode: PermissionMode::Deny,
            }]))
            .await;
        engine
            .set_session_mode(session, Permission::ShellExecute, PermissionMode::Allow)
            .await;
        engine
            .set_path_grant(
                Permission::ShellExecute,
                "workspace",
                PermissionMode::Allow,
                Some(session),
                None,
            )
            .await;
        assert_eq!(
            engine
                .check_scoped(
                    Permission::ShellExecute,
                    &PermissionCheck {
                        session_id: Some(session),
                        path: Some("workspace"),
                        command: Some("rm -rf target"),
                    },
                )
                .await,
            PermissionDecision::Denied
        );
    });
}

#[test]
fn policy_deny_overrides_path_grant() {
    block_on(async {
        let session = Uuid::new_v4();
        let engine = PermissionEngine::new();
        engine
            .set_policy_rules(PolicyRuleSet::new(vec![PolicyRule {
                permission: Permission::ShellExecute,
                pattern: "rm *".into(),
                mode: PermissionMode::Deny,
            }]))
            .await;
        engine
            .set_path_grant(
                Permission::ShellExecute,
                "workspace",
                PermissionMode::Allow,
                Some(session),
                None,
            )
            .await;
        assert_eq!(
            engine
                .check_scoped(
                    Permission::ShellExecute,
                    &PermissionCheck {
                        session_id: Some(session),
                        path: Some("workspace"),
                        command: Some("rm -rf /"),
                    },
                )
                .await,
            PermissionDecision::Denied
        );
    });
}

#[test]
fn policy_deny_overrides_session_mode() {
    block_on(async {
        let session = Uuid::new_v4();
        let engine = PermissionEngine::new();
        engine
            .set_policy_rules(PolicyRuleSet::new(vec![PolicyRule {
                permission: Permission::ShellExecute,
                pattern: "rm *".into(),
                mode: PermissionMode::Deny,
            }]))
            .await;
        engine
            .set_session_mode(session, Permission::ShellExecute, PermissionMode::Allow)
            .await;
        assert_eq!(
            engine
                .check_scoped(
                    Permission::ShellExecute,
                    &PermissionCheck {
                        session_id: Some(session),
                        path: None,
                        command: Some("rm target/debug"),
                    },
                )
                .await,
            PermissionDecision::Denied
        );
    });
}

#[test]
fn path_grant_beats_allow_policy() {
    block_on(async {
        let engine = PermissionEngine::new();
        engine
            .set_policy_rules(PolicyRuleSet::new(vec![PolicyRule {
                permission: Permission::FilesystemWrite,
                pattern: "docs/*".into(),
                mode: PermissionMode::Allow,
            }]))
            .await;
        // Global mode is Ask for FilesystemWrite
        assert_eq!(
            engine.mode(Permission::FilesystemWrite).await,
            PermissionMode::Ask
        );
        // Policy rule says Allow
        assert_eq!(
            engine
                .check_scoped(
                    Permission::FilesystemWrite,
                    &PermissionCheck {
                        session_id: None,
                        path: Some("docs/readme.md"),
                        command: None,
                    },
                )
                .await,
            PermissionDecision::Allowed
        );
        // But matching path grant takes precedence
        let session = Uuid::new_v4();
        engine
            .set_path_grant(
                Permission::FilesystemWrite,
                "docs",
                PermissionMode::Deny,
                Some(session),
                None,
            )
            .await;
        assert_eq!(
            engine
                .check_scoped(
                    Permission::FilesystemWrite,
                    &PermissionCheck {
                        session_id: Some(session),
                        path: Some("docs/readme.md"),
                        command: None,
                    },
                )
                .await,
            PermissionDecision::Denied
        );
    });
}

#[test]
fn missing_policy_rule_uses_global_mode() {
    block_on(async {
        let engine = PermissionEngine::new();
        engine
            .set_policy_rules(PolicyRuleSet::new(vec![PolicyRule {
                permission: Permission::FilesystemWrite,
                pattern: "secrets/*".into(),
                mode: PermissionMode::Deny,
            }]))
            .await;
        // Matching rule denies
        assert_eq!(
            engine
                .check_scoped(
                    Permission::FilesystemWrite,
                    &PermissionCheck {
                        session_id: None,
                        path: Some("secrets/key.env"),
                        command: None,
                    },
                )
                .await,
            PermissionDecision::Denied
        );
        // Non-matching path falls back to global mode (Ask for FilesystemWrite)
        assert_eq!(
            engine
                .check_scoped(
                    Permission::FilesystemWrite,
                    &PermissionCheck {
                        session_id: None,
                        path: Some("src/main.rs"),
                        command: None,
                    },
                )
                .await,
            PermissionDecision::NeedsApproval
        );
    });
}

#[test]
fn command_field_defaults_to_none_in_existing_checks() {
    block_on(async {
        let engine = PermissionEngine::new();
        let session = Uuid::new_v4();
        engine
            .set_path_grant(
                Permission::FilesystemWrite,
                "docs",
                PermissionMode::Allow,
                Some(session),
                None,
            )
            .await;
        // Existing code using PermissionCheck without command should still work
        let check = PermissionCheck {
            session_id: Some(session),
            path: Some("docs/readme.md"),
            // command is not specified, defaults to None
            ..Default::default()
        };
        assert_eq!(
            engine
                .check_scoped(Permission::FilesystemWrite, &check)
                .await,
            PermissionDecision::Allowed
        );
    });
}

#[test]
fn canonical_policy_subject_cannot_be_replaced_by_display_name() {
    block_on(async {
        let engine = PermissionEngine::new();
        engine
            .set_policy_rules(PolicyRuleSet::new(vec![PolicyRule {
                permission: Permission::FilesystemRead,
                pattern: "c:/workspace/secrets/*".into(),
                mode: PermissionMode::Deny,
            }]))
            .await;

        let check = PermissionCheck {
            path: Some("friendly-name/token.txt"),
            ..PermissionCheck::default()
        };
        assert_eq!(
            engine
                .check_scoped_with_subject(
                    Permission::FilesystemRead,
                    &check,
                    r"C:\workspace\secrets\token.txt",
                )
                .await,
            PermissionDecision::Denied
        );
        assert_eq!(
            engine
                .check_scoped(Permission::FilesystemRead, &check)
                .await,
            PermissionDecision::Allowed
        );
    });
}

#[test]
fn approval_matches_detects_different_inputs() {
    block_on(async {
        let engine = PermissionEngine::new();
        let task_id = Uuid::new_v4();
        let input1 = serde_json::json!({"cmd": "cargo", "args": ["--version"]});
        let input2 = serde_json::json!({"cmd": "cargo", "args": ["publish"]});

        let request = engine
            .create_approval_scoped_for_call(
                task_id,
                None,
                "shell.execute",
                Permission::ShellExecute,
                "workspace",
                &input1,
            )
            .await;
        engine.resolve(request.id, true).await.expect("granted");

        // Same hash matches
        assert!(
            engine
                .approval_matches(request.id, &request.call_hash)
                .await
        );

        // Different input has different hash
        let different_hash = canonical_call_hash("shell.execute", "workspace", &input2);
        assert_ne!(request.call_hash, different_hash);
        assert!(!engine.approval_matches(request.id, &different_hash).await);
    });
}

#[test]
fn approval_matches_rejects_denied_approval() {
    block_on(async {
        let engine = PermissionEngine::new();
        let task_id = Uuid::new_v4();
        let input = serde_json::json!({"cmd": "rm", "args": ["-rf", "/"]});

        let request = engine
            .create_approval_scoped_for_call(
                task_id,
                None,
                "shell.execute",
                Permission::ShellExecute,
                "workspace",
                &input,
            )
            .await;
        engine.resolve(request.id, false).await.expect("denied");

        // Denied approval should not match even with correct hash
        assert!(
            !engine
                .approval_matches(request.id, &request.call_hash)
                .await
        );
    });
}

#[test]
fn approval_matches_rejects_pending_approval() {
    block_on(async {
        let engine = PermissionEngine::new();
        let task_id = Uuid::new_v4();
        let input = serde_json::json!({"path": "file.txt", "content": "hello"});

        let request = engine
            .create_approval_scoped_for_call(
                task_id,
                None,
                "filesystem.write",
                Permission::FilesystemWrite,
                "file.txt",
                &input,
            )
            .await;

        // Pending approval should not match
        assert!(
            !engine
                .approval_matches(request.id, &request.call_hash)
                .await
        );
    });
}

#[test]
fn approval_for_git_commit_rejects_different_message() {
    block_on(async {
        let engine = PermissionEngine::new();
        let task_id = Uuid::new_v4();
        let input1 = serde_json::json!({"message": "feat: add new feature"});
        let input2 = serde_json::json!({"message": "fix: repair broken thing"});

        let request = engine
            .create_approval_scoped_for_call(
                task_id,
                None,
                "git.commit",
                Permission::GitWrite,
                "workspace",
                &input1,
            )
            .await;
        engine.resolve(request.id, true).await.expect("granted");

        // Different commit message has different hash
        let different_hash = canonical_call_hash("git.commit", "workspace", &input2);
        assert!(!engine.approval_matches(request.id, &different_hash).await);
    });
}

#[test]
fn approval_for_file_write_rejects_different_content() {
    block_on(async {
        let engine = PermissionEngine::new();
        let task_id = Uuid::new_v4();
        let input1 = serde_json::json!({"path": "src/main.rs", "content": "fn main() {}"});
        let input2 =
            serde_json::json!({"path": "src/main.rs", "content": "fn main() { panic!(); }"});

        let request = engine
            .create_approval_scoped_for_call(
                task_id,
                None,
                "filesystem.write",
                Permission::FilesystemWrite,
                "src/main.rs",
                &input1,
            )
            .await;
        engine.resolve(request.id, true).await.expect("granted");

        // Same path but different content should not match
        let different_hash = canonical_call_hash("filesystem.write", "src/main.rs", &input2);
        assert!(!engine.approval_matches(request.id, &different_hash).await);
    });
}

#[test]
fn call_hash_is_stable_across_json_key_order() {
    block_on(async {
        let engine = PermissionEngine::new();
        let task_id = Uuid::new_v4();
        let input1 = serde_json::json!({"b": 2, "a": 1, "c": {"z": 26, "x": 24}});
        let input2 = serde_json::json!({"c": {"x": 24, "z": 26}, "a": 1, "b": 2});

        let request = engine
            .create_approval_scoped_for_call(
                task_id,
                None,
                "mcp.call",
                Permission::McpCall,
                "workspace",
                &input1,
            )
            .await;
        engine.resolve(request.id, true).await.expect("granted");

        // Same input with different key order should have same hash
        let equivalent_hash = canonical_call_hash("mcp.call", "workspace", &input2);
        assert_eq!(request.call_hash, equivalent_hash);
        assert!(engine.approval_matches(request.id, &equivalent_hash).await);
    });
}

#[test]
fn hard_deny_policy_stops_granted_approval_execution() {
    block_on(async {
        let engine = PermissionEngine::new();
        let task_id = Uuid::new_v4();
        let input = serde_json::json!({"cmd": "rm -rf /"});

        // Create and grant approval
        let request = engine
            .create_approval_scoped_for_call(
                task_id,
                None,
                "shell.execute",
                Permission::ShellExecute,
                "workspace",
                &input,
            )
            .await;
        engine.resolve(request.id, true).await.expect("granted");

        // Approval matches initially
        assert!(
            engine
                .approval_matches(request.id, &request.call_hash)
                .await
        );

        // Now add a hard deny policy for this command
        engine
            .set_policy_rules(PolicyRuleSet::new(vec![PolicyRule {
                permission: Permission::ShellExecute,
                pattern: "rm *".into(),
                mode: PermissionMode::Deny,
            }]))
            .await;

        // Check with same input still shows approval matches
        // (approval_matches only checks approval state, not policy)
        assert!(
            engine
                .approval_matches(request.id, &request.call_hash)
                .await
        );

        // But check_scoped now denies due to hard policy
        let check = PermissionCheck {
            session_id: None,
            path: Some("workspace"),
            command: Some("rm -rf /"),
        };
        assert_eq!(
            engine.check_scoped(Permission::ShellExecute, &check).await,
            PermissionDecision::Denied
        );
    });
}

#[test]
fn approval_scope_normalization_consistent_with_hash() {
    block_on(async {
        let engine = PermissionEngine::new();
        let task_id = Uuid::new_v4();
        let input = serde_json::json!({"content": "test"});

        // Create approval with backslashes
        let request = engine
            .create_approval_scoped_for_call(
                task_id,
                None,
                "filesystem.write",
                Permission::FilesystemWrite,
                r".\src\main.rs",
                &input,
            )
            .await;
        engine.resolve(request.id, true).await.expect("granted");

        // The request should have normalized scope (converted to forward slashes)
        assert!(request.scope.contains("src/main.rs"));

        // And hash should match
        let expected_hash = canonical_call_hash("filesystem.write", &request.scope, &input);
        assert_eq!(request.call_hash, expected_hash);
        assert!(engine.approval_matches(request.id, &expected_hash).await);
    });
}

#[test]
fn approval_matches_returns_false_for_nonexistent_id() {
    block_on(async {
        let engine = PermissionEngine::new();
        let fake_id = Uuid::new_v4();
        let call_hash = "some_hash".to_string();

        assert!(!engine.approval_matches(fake_id, &call_hash).await);
    });
}

#[test]
fn extended_windows_path_prefix_matches_regular_drive_subject() {
    assert_eq!(
        normalize_scope_path("//?/C:/workspace/secrets/token.txt"),
        "C:/workspace/secrets/token.txt"
    );
}

#[test]
fn fingerprint_null_and_empty_string() {
    assert_eq!(fingerprint_input(&serde_json::Value::Null), "null");
    assert_eq!(fingerprint_input(&serde_json::json!("")), "\"\"");
}

#[test]
fn fingerprint_bool() {
    assert_eq!(fingerprint_input(&serde_json::json!(true)), "true");
    assert_eq!(fingerprint_input(&serde_json::json!(false)), "false");
}

#[test]
fn fingerprint_int64_within_safe_bound_is_a_bare_number() {
    assert_eq!(
        fingerprint_input(&serde_json::json!(9_007_199_254_740_991_i64)),
        "9007199254740991"
    );
    assert_eq!(
        fingerprint_input(&serde_json::json!(-9_007_199_254_740_991_i64)),
        "-9007199254740991"
    );
    assert_eq!(fingerprint_input(&serde_json::json!(0)), "0");
}

#[test]
fn fingerprint_int64_outside_safe_bound_is_typed_object() {
    assert_eq!(
        fingerprint_input(&serde_json::json!(9_007_199_254_740_992_i64)),
        "{\"type\":\"int64\",\"value\":\"9007199254740992\"}"
    );
    assert_eq!(
        fingerprint_input(&serde_json::json!(-9_007_199_254_740_992_i64)),
        "{\"type\":\"int64\",\"value\":\"-9007199254740992\"}"
    );
    assert_eq!(
        fingerprint_input(&serde_json::json!(u64::MAX)),
        format!("{{\"type\":\"int64\",\"value\":\"{}\"}}", u64::MAX)
    );
}

#[test]
fn fingerprint_float_uses_number_representation() {
    assert_eq!(fingerprint_input(&serde_json::json!(0.1)), "0.1");
    assert_eq!(fingerprint_input(&serde_json::json!(1.5)), "1.5");
}

#[test]
fn fingerprint_unicode_text_preserves_string_type() {
    assert_eq!(fingerprint_input(&serde_json::json!("héllo")), "\"héllo\"");
    assert_eq!(fingerprint_input(&serde_json::json!("🦀")), "\"🦀\"");
}

#[test]
fn fingerprint_object_key_order_is_insertion_independent() {
    let a = serde_json::json!({"b": 1, "a": 2});
    let b = serde_json::json!({"a": 2, "b": 1});
    assert_eq!(fingerprint_input(&a), fingerprint_input(&b));
    assert_eq!(fingerprint_input(&a), "{\"a\":2,\"b\":1}");
}

#[test]
fn fingerprint_array_order_is_significant() {
    let a = serde_json::json!([1, 2]);
    let b = serde_json::json!([2, 1]);
    assert_ne!(fingerprint_input(&a), fingerprint_input(&b));
}

#[test]
fn fingerprint_pre_encoded_bytes_object_round_trips_as_plain_object() {
    let value = serde_json::json!({"type": "bytes", "encoding": "base64url", "value": "aGVsbG8"});
    assert_eq!(
        fingerprint_input(&value),
        "{\"type\":\"bytes\",\"encoding\":\"base64url\",\"value\":\"aGVsbG8\"}"
    );
}

#[test]
fn canonical_call_hash_is_stable_across_key_order() {
    let a = serde_json::json!({"b": 1, "a": 2});
    let b = serde_json::json!({"a": 2, "b": 1});
    assert_eq!(
        canonical_call_hash("tool", "scope", &a),
        canonical_call_hash("tool", "scope", &b)
    );
}

#[test]
fn normalize_scope_rejects_embedded_newline() {
    block_on(async {
        let engine = PermissionEngine::new();
        assert!(engine.normalize_scope("a\nb").is_err());
    });
}

#[test]
fn normalize_scope_matches_normalize_scope_path_for_paths() {
    block_on(async {
        let engine = PermissionEngine::new();
        assert_eq!(
            engine
                .normalize_scope("//?/C:/workspace/secrets/token.txt")
                .unwrap(),
            normalize_scope_path("//?/C:/workspace/secrets/token.txt")
        );
    });
}

#[test]
fn normalize_scope_leaves_non_path_scope_trimmed() {
    block_on(async {
        let engine = PermissionEngine::new();
        assert_eq!(
            engine.normalize_scope("  https://example.com  ").unwrap(),
            "https://example.com"
        );
    });
}

#[test]
fn normalize_scope_is_idempotent() {
    block_on(async {
        let engine = PermissionEngine::new();
        let once = engine.normalize_scope("src\\lib.rs").unwrap();
        let twice = engine.normalize_scope(&once).unwrap();
        assert_eq!(once, twice);
    });
}

#[test]
fn microphone_listen_is_present_and_denied_by_default() {
    block_on(async {
        let engine = PermissionEngine::new();
        // Presence matters: a missing key would fall back to `Ask`.
        assert!(engine
            .modes
            .read()
            .await
            .contains_key(&Permission::MicrophoneListen));
        assert_eq!(
            engine.mode(Permission::MicrophoneListen).await,
            PermissionMode::Deny
        );
        assert_eq!(
            engine.check(Permission::MicrophoneListen).await,
            PermissionDecision::Denied
        );
    });
}

#[test]
fn set_all_modes_never_touches_microphone_listen() {
    block_on(async {
        for mode in [
            PermissionMode::Allow,
            PermissionMode::Deny,
            PermissionMode::Ask,
        ] {
            let engine = PermissionEngine::new();
            engine.set_all_modes(mode).await;
            assert_eq!(
                engine.mode(Permission::MicrophoneListen).await,
                PermissionMode::Deny,
                "set_all_modes({mode:?}) changed the microphone capability"
            );
            assert_eq!(engine.mode(Permission::ShellExecute).await, mode);
        }
    });
}

#[test]
fn microphone_listen_stays_denied_across_repeated_workspace_opens() {
    block_on(async {
        let engine = PermissionEngine::new();
        for mode in [
            PermissionMode::Allow,
            PermissionMode::Ask,
            PermissionMode::Allow,
            PermissionMode::Deny,
        ] {
            engine.set_all_modes(mode).await;
        }
        assert_eq!(
            engine.mode(Permission::MicrophoneListen).await,
            PermissionMode::Deny
        );
    });
}

#[test]
fn microphone_listen_is_granted_only_by_an_explicit_named_call() {
    block_on(async {
        let engine = PermissionEngine::new();
        engine
            .set_mode(Permission::MicrophoneListen, PermissionMode::Allow)
            .await;
        assert_eq!(
            engine.check(Permission::MicrophoneListen).await,
            PermissionDecision::Allowed
        );
    });
}

#[test]
fn microphone_listen_serializes_as_snake_case() {
    assert_eq!(
        serde_json::to_string(&Permission::MicrophoneListen).unwrap(),
        "\"microphone_listen\""
    );
    assert_eq!(
        serde_json::from_str::<Permission>("\"microphone_listen\"").unwrap(),
        Permission::MicrophoneListen
    );
}
