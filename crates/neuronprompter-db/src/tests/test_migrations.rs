// =============================================================================
// Migration chain tests.
//
// Verifies that the initial migration creates all expected tables, indexes,
// and the FTS5 virtual table with functional MATCH queries.
// =============================================================================

use super::setup_db;
use crate::ConnectionProvider;

#[test]
fn migration_creates_all_tables() {
    let db = setup_db();
    db.with_connection(|conn| {
        let tables: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT name FROM sqlite_master WHERE type = 'table' \
                     AND name NOT LIKE 'sqlite_%' \
                     AND name NOT LIKE 'schema_%' \
                     ORDER BY name",
                )
                .unwrap();
            let rows = stmt.query_map([], |row| row.get(0)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };

        // FTS5 content-sync tables (content='prompts') do not create a
        // separate _content shadow table; the remaining shadow tables are
        // _config, _data, _docsize, and _idx.
        let expected = [
            "app_settings",
            "categories",
            "collections",
            "prompt_categories",
            "prompt_collections",
            "prompt_tags",
            "prompt_versions",
            "prompts",
            "prompts_fts",        // FTS5 virtual table
            "prompts_fts_config", // FTS5 internal shadow table
            "prompts_fts_data",
            "prompts_fts_docsize",
            "prompts_fts_idx",
            "tags",
            "user_settings",
            "users",
        ];

        for table_name in &expected {
            assert!(
                tables.iter().any(|t| t == table_name),
                "table '{table_name}' missing from schema; found: {tables:?}"
            );
        }
        Ok(())
    })
    .unwrap();
}

#[test]
fn migration_creates_indexes() {
    let db = setup_db();
    db.with_connection(|conn| {
        let indexes: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT name FROM sqlite_master WHERE type = 'index' \
                     AND name NOT LIKE 'sqlite_%' \
                     ORDER BY name",
                )
                .unwrap();
            let rows = stmt.query_map([], |row| row.get(0)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };

        // Verify a representative subset of the 11 secondary indexes created
        // by the migration. Index names follow abbreviated conventions from
        // the migration SQL (e.g. idx_prompts_user, idx_pt_tag).
        let expected_prefixes = [
            "idx_prompts_user",
            "idx_pt_tag",
            "idx_pcat_category",
            "idx_pc_collection",
            "idx_versions_prompt",
        ];

        for prefix in &expected_prefixes {
            assert!(
                indexes.iter().any(|i| i.starts_with(prefix)),
                "no index with prefix '{prefix}' found; existing: {indexes:?}"
            );
        }
        Ok(())
    })
    .unwrap();
}

#[test]
fn fts5_virtual_table_responds_to_match() {
    let db = setup_db();
    db.with_connection(|conn| {
        // Insert a prompt directly so FTS triggers fire.
        conn.execute(
            "INSERT INTO users (username, display_name) VALUES ('test_user', 'Test')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO prompts (user_id, title, content) VALUES (1, 'Hello World', 'body text')",
            [],
        )
        .unwrap();

        // FTS5 MATCH query should return one result.
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM prompts_fts WHERE prompts_fts MATCH '\"Hello\"*'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "FTS5 MATCH should find the inserted prompt");
        Ok(())
    })
    .unwrap();
}

#[test]
fn migration_is_idempotent() {
    let db = setup_db();
    // Run migrations a second time; should not fail or duplicate schema.
    db.with_connection(|conn| {
        crate::migrations::run_migrations(conn)?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn schema_version_tracks_applied_migrations() {
    let db = setup_db();
    db.with_connection(|conn| {
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(
            version >= 1,
            "schema_version should record at least version 1"
        );
        Ok(())
    })
    .unwrap();
}
