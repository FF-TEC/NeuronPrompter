// =============================================================================
// Schema constraint integration tests.
//
// Verifies that database-level constraints enforce data integrity:
// - Cross-user ownership triggers on junction tables and chain_steps
// - Case-insensitive uniqueness on taxonomy names
// - JSON validity on user_settings.extra
// - Value-domain CHECK constraints
// - Non-empty title/name enforcement
// =============================================================================

use neuronprompter_core::domain::user::NewUser;

use super::setup_db;
use crate::ConnectionProvider;
use crate::repo::{categories, collections, prompts, scripts, settings, tags, users};

fn create_user(conn: &rusqlite::Connection, username: &str) -> i64 {
    let new = NewUser {
        username: username.to_owned(),
        display_name: format!("Display {username}"),
    };
    users::create_user(conn, &new).expect("user creation").id
}

// ---------------------------------------------------------------------------
// Cross-user ownership triggers
// ---------------------------------------------------------------------------

#[test]
fn prompt_tag_ownership_mismatch_rejected() {
    let db = setup_db();
    db.with_connection(|conn| {
        let user_a = create_user(conn, "alice");
        let user_b = create_user(conn, "bob");

        let prompt = prompts::create_prompt(
            conn,
            &neuronprompter_core::domain::prompt::NewPrompt {
                user_id: user_a,
                title: "A's prompt".to_owned(),
                content: "content".to_owned(),
                description: None,
                notes: None,
                language: None,
                tag_ids: vec![],
                category_ids: vec![],
                collection_ids: vec![],
            },
        )?;

        let tag_b = tags::create_tag(conn, user_b, "bob_tag")?;

        // Attempt cross-user link: A's prompt + B's tag
        let result = tags::link_prompt_tag(conn, prompt.id, tag_b.id);
        assert!(
            result.is_err(),
            "cross-user prompt_tag link should be rejected"
        );
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains("ownership mismatch"),
            "error should mention ownership: {err_msg}"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn prompt_category_ownership_mismatch_rejected() {
    let db = setup_db();
    db.with_connection(|conn| {
        let user_a = create_user(conn, "alice");
        let user_b = create_user(conn, "bob");

        let prompt = prompts::create_prompt(
            conn,
            &neuronprompter_core::domain::prompt::NewPrompt {
                user_id: user_a,
                title: "A's prompt".to_owned(),
                content: "content".to_owned(),
                description: None,
                notes: None,
                language: None,
                tag_ids: vec![],
                category_ids: vec![],
                collection_ids: vec![],
            },
        )?;

        let cat_b = categories::create_category(conn, user_b, "bob_cat")?;

        let result = categories::link_prompt_category(conn, prompt.id, cat_b.id);
        assert!(
            result.is_err(),
            "cross-user prompt_category link should be rejected"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn prompt_collection_ownership_mismatch_rejected() {
    let db = setup_db();
    db.with_connection(|conn| {
        let user_a = create_user(conn, "alice");
        let user_b = create_user(conn, "bob");

        let prompt = prompts::create_prompt(
            conn,
            &neuronprompter_core::domain::prompt::NewPrompt {
                user_id: user_a,
                title: "A's prompt".to_owned(),
                content: "content".to_owned(),
                description: None,
                notes: None,
                language: None,
                tag_ids: vec![],
                category_ids: vec![],
                collection_ids: vec![],
            },
        )?;

        let col_b = collections::create_collection(conn, user_b, "bob_col")?;

        let result = collections::link_prompt_collection(conn, prompt.id, col_b.id);
        assert!(
            result.is_err(),
            "cross-user prompt_collection link should be rejected"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn script_tag_ownership_mismatch_rejected() {
    let db = setup_db();
    db.with_connection(|conn| {
        let user_a = create_user(conn, "alice");
        let user_b = create_user(conn, "bob");

        let script = scripts::create_script(
            conn,
            &neuronprompter_core::domain::script::NewScript {
                user_id: user_a,
                title: "a_script.py".to_owned(),
                content: "print()".to_owned(),
                description: None,
                notes: None,
                script_language: "python".to_owned(),
                language: None,
                source_path: None,
                is_synced: false,
                tag_ids: vec![],
                category_ids: vec![],
                collection_ids: vec![],
            },
        )?;

        let tag_b = tags::create_tag(conn, user_b, "bob_tag")?;

        let result = scripts::link_script_tag(conn, script.id, tag_b.id);
        assert!(
            result.is_err(),
            "cross-user script_tag link should be rejected"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn same_user_prompt_tag_allowed() {
    let db = setup_db();
    db.with_connection(|conn| {
        let user_a = create_user(conn, "alice");

        let prompt = prompts::create_prompt(
            conn,
            &neuronprompter_core::domain::prompt::NewPrompt {
                user_id: user_a,
                title: "A's prompt".to_owned(),
                content: "content".to_owned(),
                description: None,
                notes: None,
                language: None,
                tag_ids: vec![],
                category_ids: vec![],
                collection_ids: vec![],
            },
        )?;

        let tag_a = tags::create_tag(conn, user_a, "alice_tag")?;

        // Same-user link should succeed
        tags::link_prompt_tag(conn, prompt.id, tag_a.id)?;
        Ok(())
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// COLLATE NOCASE on taxonomy names
// ---------------------------------------------------------------------------

#[test]
fn tag_name_case_insensitive_unique() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "alice");

        tags::create_tag(conn, uid, "Work")?;
        let result = tags::create_tag(conn, uid, "work");
        assert!(
            result.is_err(),
            "case-variant tag name should violate UNIQUE constraint"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn category_name_case_insensitive_unique() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "alice");

        categories::create_category(conn, uid, "Development")?;
        let result = categories::create_category(conn, uid, "development");
        assert!(
            result.is_err(),
            "case-variant category name should violate UNIQUE constraint"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn collection_name_case_insensitive_unique() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "alice");

        collections::create_collection(conn, uid, "Templates")?;
        let result = collections::create_collection(conn, uid, "templates");
        assert!(
            result.is_err(),
            "case-variant collection name should violate UNIQUE constraint"
        );
        Ok(())
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// json_valid CHECK on extra field
// ---------------------------------------------------------------------------

#[test]
fn user_settings_extra_rejects_invalid_json() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "alice");
        settings::create_default_user_settings(conn, uid)?;

        // Try to set extra to invalid JSON via raw SQL
        let result = conn.execute(
            "UPDATE user_settings SET extra = 'not json' WHERE user_id = ?1",
            [uid],
        );
        assert!(
            result.is_err(),
            "invalid JSON in extra should be rejected by CHECK"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn user_settings_extra_accepts_valid_json() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "alice");
        settings::create_default_user_settings(conn, uid)?;

        conn.execute(
            "UPDATE user_settings SET extra = '{\"custom\": true}' WHERE user_id = ?1",
            [uid],
        )
        .expect("valid JSON should be accepted");
        Ok(())
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// CHECK constraints on value-domain fields
// ---------------------------------------------------------------------------

#[test]
fn user_settings_theme_check_rejects_invalid() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "alice");
        settings::create_default_user_settings(conn, uid)?;

        let result = conn.execute(
            "UPDATE user_settings SET theme = 'blue' WHERE user_id = ?1",
            [uid],
        );
        assert!(result.is_err(), "invalid theme value should be rejected");
        Ok(())
    })
    .unwrap();
}

#[test]
fn user_settings_sort_direction_check_rejects_invalid() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "alice");
        settings::create_default_user_settings(conn, uid)?;

        let result = conn.execute(
            "UPDATE user_settings SET sort_direction = 'random' WHERE user_id = ?1",
            [uid],
        );
        assert!(result.is_err(), "invalid sort_direction should be rejected");
        Ok(())
    })
    .unwrap();
}

#[test]
fn chain_step_position_rejects_negative() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "alice");
        let prompt = prompts::create_prompt(
            conn,
            &neuronprompter_core::domain::prompt::NewPrompt {
                user_id: uid,
                title: "test".to_owned(),
                content: "content".to_owned(),
                description: None,
                notes: None,
                language: None,
                tag_ids: vec![],
                category_ids: vec![],
                collection_ids: vec![],
            },
        )?;

        let chain = conn.query_row(
            "INSERT INTO chains (user_id, title) VALUES (?1, 'test chain') RETURNING id",
            [uid],
            |row| row.get::<_, i64>(0),
        ).map_err(|e| crate::DbError::Query { operation: "test".to_owned(), source: e })?;

        let result = conn.execute(
            "INSERT INTO chain_steps (chain_id, step_type, prompt_id, position) VALUES (?1, 'prompt', ?2, -1)",
            rusqlite::params![chain, prompt.id],
        );
        assert!(result.is_err(), "negative position should be rejected");
        Ok(())
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// Non-empty title/name checks
// ---------------------------------------------------------------------------

#[test]
fn prompt_rejects_empty_title() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "alice");

        let result = conn.execute(
            "INSERT INTO prompts (user_id, title, content) VALUES (?1, '', 'content')",
            [uid],
        );
        assert!(result.is_err(), "empty prompt title should be rejected");

        let result2 = conn.execute(
            "INSERT INTO prompts (user_id, title, content) VALUES (?1, '   ', 'content')",
            [uid],
        );
        assert!(
            result2.is_err(),
            "whitespace-only prompt title should be rejected"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn tag_rejects_empty_name() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "alice");

        let result = conn.execute("INSERT INTO tags (user_id, name) VALUES (?1, '')", [uid]);
        assert!(result.is_err(), "empty tag name should be rejected");

        let result2 = conn.execute("INSERT INTO tags (user_id, name) VALUES (?1, '   ')", [uid]);
        assert!(
            result2.is_err(),
            "whitespace-only tag name should be rejected"
        );
        Ok(())
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// UPDATE ownership triggers (defense-in-depth)
// ---------------------------------------------------------------------------

#[test]
fn prompt_tag_update_ownership_mismatch_rejected() {
    let db = setup_db();
    db.with_connection(|conn| {
        let user_a = create_user(conn, "alice");
        let user_b = create_user(conn, "bob");

        let prompt_a = prompts::create_prompt(
            conn,
            &neuronprompter_core::domain::prompt::NewPrompt {
                user_id: user_a,
                title: "A".to_owned(),
                content: "c".to_owned(),
                description: None,
                notes: None,
                language: None,
                tag_ids: vec![],
                category_ids: vec![],
                collection_ids: vec![],
            },
        )?;
        let tag_a = tags::create_tag(conn, user_a, "tag_a")?;
        let tag_b = tags::create_tag(conn, user_b, "tag_b")?;

        // Insert valid link
        tags::link_prompt_tag(conn, prompt_a.id, tag_a.id)?;

        // Try UPDATE to cross-user tag
        let result = conn.execute(
            "UPDATE prompt_tags SET tag_id = ?1 WHERE prompt_id = ?2 AND tag_id = ?3",
            rusqlite::params![tag_b.id, prompt_a.id, tag_a.id],
        );
        assert!(
            result.is_err(),
            "UPDATE to cross-user tag should be rejected"
        );
        Ok(())
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// user_settings INSERT ownership
// ---------------------------------------------------------------------------

#[test]
fn user_settings_insert_with_foreign_collection_rejected() {
    let db = setup_db();
    db.with_connection(|conn| {
        let user_a = create_user(conn, "alice");
        let user_b = create_user(conn, "bob");
        let col_b = collections::create_collection(conn, user_b, "bobs_col")?;

        // Try inserting user_settings for user_a with user_b's collection
        let result = conn.execute(
            "INSERT INTO user_settings (user_id, last_collection_id) VALUES (?1, ?2)",
            rusqlite::params![user_a, col_b.id],
        );
        assert!(
            result.is_err(),
            "INSERT with foreign collection should be rejected"
        );
        Ok(())
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// username non-empty CHECK
// ---------------------------------------------------------------------------

#[test]
fn user_rejects_empty_username() {
    let db = setup_db();
    db.with_connection(|conn| {
        let result = conn.execute(
            "INSERT INTO users (username, display_name) VALUES ('', 'Test')",
            [],
        );
        assert!(result.is_err(), "empty username should be rejected");

        let result2 = conn.execute(
            "INSERT INTO users (username, display_name) VALUES ('   ', 'Test')",
            [],
        );
        assert!(
            result2.is_err(),
            "whitespace-only username should be rejected"
        );
        Ok(())
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// current_version >= 1
// ---------------------------------------------------------------------------

#[test]
fn prompt_rejects_zero_current_version() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "alice");
        let result = conn.execute(
            "INSERT INTO prompts (user_id, title, content, current_version) VALUES (?1, 'test', 'c', 0)",
            [uid],
        );
        assert!(result.is_err(), "current_version = 0 should be rejected");
        Ok(())
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// extra must be a JSON object, not array/string/number
// ---------------------------------------------------------------------------

#[test]
fn user_settings_extra_rejects_json_array() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "alice");
        settings::create_default_user_settings(conn, uid)?;

        let result = conn.execute(
            "UPDATE user_settings SET extra = '[1,2,3]' WHERE user_id = ?1",
            [uid],
        );
        assert!(
            result.is_err(),
            "JSON array in extra should be rejected — must be object"
        );
        Ok(())
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// Version table language CHECKs
// ---------------------------------------------------------------------------

#[test]
fn prompt_version_rejects_long_language() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "alice");
        let prompt = prompts::create_prompt(
            conn,
            &neuronprompter_core::domain::prompt::NewPrompt {
                user_id: uid,
                title: "test".to_owned(),
                content: "c".to_owned(),
                description: None,
                notes: None,
                language: None,
                tag_ids: vec![],
                category_ids: vec![],
                collection_ids: vec![],
            },
        )?;

        let result = conn.execute(
            "INSERT INTO prompt_versions (prompt_id, version_number, title, content, language) \
             VALUES (?1, 1, 'test', 'c', 'toolong')",
            [prompt.id],
        );
        assert!(
            result.is_err(),
            "language > 2 chars in prompt_versions should be rejected"
        );
        Ok(())
    })
    .unwrap();
}
