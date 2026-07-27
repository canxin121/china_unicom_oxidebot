use sea_orm_migration::{MigrationTrait, MigratorTrait, async_trait};

mod create_config_table;
mod create_daily_table;
mod create_last_table;
mod create_multi_account_tables;
mod upgrade_usage_state;
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(create_config_table::Migration),
            Box::new(create_last_table::Migration),
            Box::new(create_daily_table::Migration),
            Box::new(upgrade_usage_state::Migration),
            Box::new(create_multi_account_tables::Migration),
        ]
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
    use sea_orm_migration::{MigrationTrait, MigratorTrait, SchemaManager};

    use super::Migrator;

    struct V2Migrator;

    #[async_trait::async_trait]
    impl MigratorTrait for V2Migrator {
        fn migrations() -> Vec<Box<dyn MigrationTrait>> {
            vec![
                Box::new(super::create_config_table::Migration),
                Box::new(super::create_last_table::Migration),
                Box::new(super::create_daily_table::Migration),
                Box::new(super::upgrade_usage_state::Migration),
            ]
        }
    }

    #[tokio::test]
    async fn fresh_database_has_multi_account_schema() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        let manager = SchemaManager::new(&db);
        assert!(manager.has_column("config", "query_mode").await.unwrap());
        assert!(manager.has_table("usage_state").await.unwrap());
        assert!(manager.has_table("unicom_account").await.unwrap());
        assert!(manager.has_table("unicom_account_state").await.unwrap());

        db.execute_unprepared(
            r#"
            INSERT INTO unicom_account VALUES
                ('owner', 'main', 'bot', '主卡', 'token-1', 'app-1', 'JUT=1',
                 '2026-07-27T00:00:00Z', NULL, 1, 300, 1800, NULL, 0.05, 'auto', 12.0),
                ('owner', 'backup', 'bot', '副卡', 'token-2', 'app-2', 'JUT=2',
                 '2026-07-27T00:00:00Z', NULL, 1, 300, 1800, NULL, 0.05, 'auto', 12.0)
            "#,
        )
        .await
        .unwrap();
        db.execute_unprepared(
            r#"
            INSERT INTO unicom_account_state VALUES
                ('owner', 'main', '{"captured_at":"one"}', '{"captured_at":"day-one"}'),
                ('owner', 'backup', '{"captured_at":"two"}', '{"captured_at":"day-two"}')
            "#,
        )
        .await
        .unwrap();
        let count = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM unicom_account WHERE owner = 'owner'",
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get::<i64>("", "count")
            .unwrap();
        assert_eq!(count, 2);
        let state_count = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM unicom_account_state WHERE owner = 'owner'",
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get::<i64>("", "count")
            .unwrap();
        assert_eq!(state_count, 2);
    }

    #[tokio::test]
    async fn legacy_tables_are_upgraded_without_dropping_credentials() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            r#"CREATE TABLE config (
                user TEXT PRIMARY KEY NOT NULL,
                bot TEXT NOT NULL,
                cookie TEXT NOT NULL,
                enable_task INTEGER NOT NULL,
                interval INTEGER NOT NULL,
                timeout INTEGER NULL,
                free_threshold REAL NULL,
                nonfree_threshold REAL NULL,
                token_online TEXT NOT NULL,
                app_id TEXT NOT NULL
            )"#,
        ))
        .await
        .unwrap();
        db.execute_unprepared(
            r#"CREATE TABLE usage_state (
                user TEXT PRIMARY KEY NOT NULL,
                previous_snapshot TEXT NULL,
                daily_snapshot TEXT NULL
            )"#,
        )
        .await
        .unwrap();
        db.execute_unprepared(
            "INSERT INTO usage_state VALUES ('u', '{\"captured_at\":\"previous\"}', '{\"captured_at\":\"daily\"}')",
        )
        .await
        .unwrap();
        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO config VALUES ('u', 'b', 'JUT=old', 1, 300, 1800, NULL, 0.05, 'legacy-token', 'legacy-app')",
        ))
        .await
        .unwrap();

        // All old create-table migrations are idempotent, so this also represents an old database
        // whose migration metadata is unavailable during recovery.
        Migrator::up(&db, None).await.unwrap();
        let manager = SchemaManager::new(&db);
        assert!(manager.has_column("config", "query_mode").await.unwrap());
        assert!(manager.has_table("usage_state").await.unwrap());
        let legacy_row = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT cookie, token_online, app_id, query_mode FROM config WHERE user = 'u'",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            legacy_row.try_get::<String>("", "cookie").unwrap(),
            "JUT=old"
        );
        assert_eq!(
            legacy_row.try_get::<String>("", "token_online").unwrap(),
            "legacy-token"
        );
        assert_eq!(
            legacy_row.try_get::<String>("", "app_id").unwrap(),
            "legacy-app"
        );
        assert_eq!(
            legacy_row.try_get::<String>("", "query_mode").unwrap(),
            "auto"
        );

        let imported = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT account_id, cookie, token_online, app_id, query_mode FROM unicom_account WHERE owner = 'u'",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            imported.try_get::<String>("", "account_id").unwrap(),
            "default"
        );
        assert_eq!(imported.try_get::<String>("", "cookie").unwrap(), "JUT=old");
        assert_eq!(
            imported.try_get::<String>("", "token_online").unwrap(),
            "legacy-token"
        );
        assert_eq!(
            imported.try_get::<String>("", "app_id").unwrap(),
            "legacy-app"
        );
        let imported_state = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT account_id, previous_snapshot, daily_snapshot FROM unicom_account_state WHERE owner = 'u'",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            imported_state.try_get::<String>("", "account_id").unwrap(),
            "default"
        );
        assert_eq!(
            imported_state
                .try_get::<String>("", "previous_snapshot")
                .unwrap(),
            r#"{"captured_at":"previous"}"#
        );
    }

    #[tokio::test]
    async fn applied_v2_migrations_advance_to_multi_account_schema() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        V2Migrator::up(&db, None).await.unwrap();
        db.execute_unprepared(
            r#"
            INSERT INTO config (
                user, bot, cookie, enable_task, interval, timeout, free_threshold,
                nonfree_threshold, token_online, app_id, query_mode
            ) VALUES (
                'owner-v2', 'bot', 'JUT=v2', 1, 600, 3600, 0.1,
                0.2, 'refresh-v2', 'app-v2', 'modern'
            )
            "#,
        )
        .await
        .unwrap();
        db.execute_unprepared(
            "INSERT INTO usage_state VALUES ('owner-v2', '{\"captured_at\":\"p\"}', '{\"captured_at\":\"d\"}')",
        )
        .await
        .unwrap();

        Migrator::up(&db, None).await.unwrap();
        let imported = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT * FROM unicom_account WHERE owner = 'owner-v2' AND account_id = 'default'",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            imported.try_get::<String>("", "token_online").unwrap(),
            "refresh-v2"
        );
        assert_eq!(imported.try_get::<String>("", "app_id").unwrap(), "app-v2");
        assert_eq!(
            imported.try_get::<String>("", "query_mode").unwrap(),
            "modern"
        );
        assert_eq!(imported.try_get::<i64>("", "interval").unwrap(), 600);
        let state = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT * FROM unicom_account_state WHERE owner = 'owner-v2' AND account_id = 'default'",
            ))
            .await
            .unwrap();
        assert!(state.is_some());
    }
}
