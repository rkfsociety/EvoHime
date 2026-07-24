/// Integration tests for plugin install/pin/uninstall lifecycle (Stage 7.8)
#[cfg(test)]
mod plugin_lifecycle_tests {
    use evohime_storage::{
        get_installed_plugin, insert_installed_plugin, list_installed_plugins,
        mark_plugin_uninstalled, update_plugin_pin, NewInstalledPlugin,
    };
    use uuid::Uuid;

    async fn setup_pool() -> sqlx::PgPool {
        let url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL required for integration tests");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect");
        sqlx::query("TRUNCATE installed_plugins CASCADE")
            .execute(&pool)
            .await
            .ok();
        pool
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_plugin_install_and_list() {
        let pool = setup_pool().await;
        let operator_id = Uuid::nil();

        let plugin = NewInstalledPlugin {
            name: "test-plugin".to_string(),
            pinned_commit: None,
            pinned_version: None,
            signature_hash: None,
        };

        let inserted = insert_installed_plugin(&pool, operator_id, &plugin)
            .await
            .expect("insert");
        assert_eq!(inserted.name, "test-plugin");
        assert_eq!(inserted.status, "active");
        assert!(inserted.uninstalled_at.is_none());

        let listed = list_installed_plugins(&pool, operator_id)
            .await
            .expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "test-plugin");
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_plugin_uninstall_soft_delete() {
        let pool = setup_pool().await;
        let operator_id = Uuid::nil();

        let plugin = NewInstalledPlugin {
            name: "test-uninstall".to_string(),
            pinned_commit: None,
            pinned_version: None,
            signature_hash: None,
        };
        insert_installed_plugin(&pool, operator_id, &plugin)
            .await
            .expect("insert");

        let uninstalled = mark_plugin_uninstalled(&pool, operator_id, "test-uninstall")
            .await
            .expect("mark")
            .expect("found");
        assert_eq!(uninstalled.status, "uninstalled");
        assert!(uninstalled.uninstalled_at.is_some());

        let listed = list_installed_plugins(&pool, operator_id)
            .await
            .expect("list");
        assert_eq!(
            listed.len(),
            0,
            "uninstalled plugins should not appear in active list"
        );

        let retrieved = get_installed_plugin(&pool, operator_id, "test-uninstall")
            .await
            .expect("get")
            .expect("found");
        assert_eq!(
            retrieved.status, "uninstalled",
            "but DB record should still exist"
        );
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_plugin_pin_and_reactivate() {
        let pool = setup_pool().await;
        let operator_id = Uuid::nil();

        // Install
        let plugin = NewInstalledPlugin {
            name: "test-pin".to_string(),
            pinned_commit: None,
            pinned_version: None,
            signature_hash: None,
        };
        insert_installed_plugin(&pool, operator_id, &plugin)
            .await
            .expect("insert");

        // Uninstall (soft delete)
        let _ = mark_plugin_uninstalled(&pool, operator_id, "test-pin")
            .await
            .expect("mark");

        // Pin and reactivate
        let pinned = update_plugin_pin(
            &pool,
            operator_id,
            "test-pin",
            Some("abc123def456".to_string()),
            Some("1.2.3".to_string()),
        )
        .await
        .expect("pin")
        .expect("found");

        assert_eq!(pinned.status, "active");
        assert_eq!(pinned.pinned_commit, Some("abc123def456".to_string()));
        assert_eq!(pinned.pinned_version, Some("1.2.3".to_string()));
        assert!(
            pinned.uninstalled_at.is_none(),
            "reactivation clears uninstalled_at"
        );

        // Verify it appears in active list
        let listed = list_installed_plugins(&pool, operator_id)
            .await
            .expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].pinned_commit, Some("abc123def456".to_string()));
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_plugin_operator_isolation() {
        let pool = setup_pool().await;
        let operator1 = Uuid::new_v4();
        let operator2 = Uuid::new_v4();

        // Operator 1 installs
        let plugin1 = NewInstalledPlugin {
            name: "shared-plugin".to_string(),
            pinned_commit: None,
            pinned_version: None,
            signature_hash: None,
        };
        insert_installed_plugin(&pool, operator1, &plugin1)
            .await
            .expect("insert");

        // Operator 2 installs same plugin name (different scope)
        let plugin2 = NewInstalledPlugin {
            name: "shared-plugin".to_string(),
            pinned_commit: Some("op2-commit".to_string()),
            pinned_version: None,
            signature_hash: None,
        };
        insert_installed_plugin(&pool, operator2, &plugin2)
            .await
            .expect("insert");

        // Each operator sees only their version
        let list1 = list_installed_plugins(&pool, operator1)
            .await
            .expect("list");
        assert_eq!(list1[0].pinned_commit, None, "operator1 has no pin");

        let list2 = list_installed_plugins(&pool, operator2)
            .await
            .expect("list");
        assert_eq!(
            list2[0].pinned_commit,
            Some("op2-commit".to_string()),
            "operator2 has pin"
        );

        // Operator 1 uninstalls doesn't affect operator 2
        let _ = mark_plugin_uninstalled(&pool, operator1, "shared-plugin")
            .await
            .expect("mark");

        let list2_after = list_installed_plugins(&pool, operator2)
            .await
            .expect("list");
        assert_eq!(list2_after.len(), 1, "operator2 still has active plugin");
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_plugin_upsert_on_reinstall() {
        let pool = setup_pool().await;
        let operator_id = Uuid::nil();

        // First install
        let plugin = NewInstalledPlugin {
            name: "test-upsert".to_string(),
            pinned_commit: None,
            pinned_version: None,
            signature_hash: None,
        };
        let v1 = insert_installed_plugin(&pool, operator_id, &plugin)
            .await
            .expect("insert v1");
        let v1_id = v1.id;

        // Reinstall (same name, ON CONFLICT DO UPDATE)
        let plugin_v2 = NewInstalledPlugin {
            name: "test-upsert".to_string(),
            pinned_commit: Some("new-commit".to_string()),
            pinned_version: None,
            signature_hash: None,
        };
        let v2 = insert_installed_plugin(&pool, operator_id, &plugin_v2)
            .await
            .expect("insert v2");

        assert_eq!(v2.id, v1_id, "upsert reuses same ID");
        assert_eq!(v2.pinned_commit, Some("new-commit".to_string()));
        assert_eq!(v2.status, "active");

        let listed = list_installed_plugins(&pool, operator_id)
            .await
            .expect("list");
        assert_eq!(listed.len(), 1, "only one entry despite two inserts");
    }
}
