// =============================================================================
// Tag repository integration tests.
//
// Verifies CRUD operations, junction table linking/unlinking, uniqueness
// constraints, and find-by-name lookups.
// =============================================================================

use neuronprompter_core::domain::user::NewUser;

use super::setup_db;
use crate::ConnectionProvider;
use crate::repo::{tags, users};

/// Helper: creates a user and returns the id.
fn create_user(conn: &rusqlite::Connection, username: &str) -> i64 {
    let new = NewUser {
        username: username.to_owned(),
        display_name: format!("Display {username}"),
    };
    users::create_user(conn, &new).expect("user creation").id
}

#[test]
fn create_and_list_tags() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "taguser");
        tags::create_tag(conn, uid, "rust")?;
        tags::create_tag(conn, uid, "ai")?;

        let list = tags::list_tags_for_user(conn, uid)?;
        assert_eq!(list.len(), 2);
        // Ordered by name.
        assert_eq!(list[0].name, "ai");
        assert_eq!(list[1].name, "rust");
        Ok(())
    })
    .unwrap();
}

#[test]
fn duplicate_tag_name_returns_error() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "dupuser");
        tags::create_tag(conn, uid, "duplicate")?;
        let result = tags::create_tag(conn, uid, "duplicate");
        assert!(result.is_err(), "duplicate tag name should fail");
        Ok(())
    })
    .unwrap();
}

#[test]
fn same_tag_name_different_users_allowed() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid1 = create_user(conn, "user_a");
        let uid2 = create_user(conn, "user_b");
        tags::create_tag(conn, uid1, "shared_name")?;
        let result = tags::create_tag(conn, uid2, "shared_name");
        assert!(
            result.is_ok(),
            "same tag name under different users should succeed"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn rename_tag() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "renameuser");
        let tag = tags::create_tag(conn, uid, "old_name")?;
        tags::rename_tag(conn, tag.id, "new_name")?;

        let found = tags::find_tag_by_name(conn, uid, "new_name")?;
        assert!(found.is_some());

        let old = tags::find_tag_by_name(conn, uid, "old_name")?;
        assert!(old.is_none());
        Ok(())
    })
    .unwrap();
}

#[test]
fn rename_tag_to_existing_name_fails() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "rename_dup_user");
        tags::create_tag(conn, uid, "name_a")?;
        let tag_b = tags::create_tag(conn, uid, "name_b")?;

        let result = tags::rename_tag(conn, tag_b.id, "name_a");
        assert!(result.is_err(), "renaming to existing name should fail");
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_tag() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "deltaguser");
        let tag = tags::create_tag(conn, uid, "ephemeral")?;
        tags::delete_tag(conn, tag.id)?;

        let list = tags::list_tags_for_user(conn, uid)?;
        assert!(list.is_empty());
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_nonexistent_tag_returns_not_found() {
    let db = setup_db();
    db.with_connection(|conn| {
        let result = tags::delete_tag(conn, 9999);
        assert!(result.is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn link_and_get_tags_for_prompt() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "linkuser");
        let tag1 = tags::create_tag(conn, uid, "tag_alpha")?;
        let tag2 = tags::create_tag(conn, uid, "tag_beta")?;

        // Create a prompt directly.
        conn.execute(
            "INSERT INTO prompts (user_id, title, content) VALUES (?1, 'test', 'content')",
            [uid],
        )
        .unwrap();
        let prompt_id = conn.last_insert_rowid();

        tags::link_prompt_tag(conn, prompt_id, tag1.id)?;
        tags::link_prompt_tag(conn, prompt_id, tag2.id)?;

        let prompt_tags = tags::get_tags_for_prompt(conn, prompt_id)?;
        assert_eq!(prompt_tags.len(), 2);
        Ok(())
    })
    .unwrap();
}

#[test]
fn link_is_idempotent() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "idempotent_user");
        let tag = tags::create_tag(conn, uid, "idem_tag")?;

        conn.execute(
            "INSERT INTO prompts (user_id, title, content) VALUES (?1, 'test', 'c')",
            [uid],
        )
        .unwrap();
        let prompt_id = conn.last_insert_rowid();

        // Link twice; second should silently succeed.
        tags::link_prompt_tag(conn, prompt_id, tag.id)?;
        tags::link_prompt_tag(conn, prompt_id, tag.id)?;

        let prompt_tags = tags::get_tags_for_prompt(conn, prompt_id)?;
        assert_eq!(
            prompt_tags.len(),
            1,
            "duplicate link should not create second row"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn unlink_removes_junction_row() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "unlinkuser");
        let tag = tags::create_tag(conn, uid, "unlink_tag")?;

        conn.execute(
            "INSERT INTO prompts (user_id, title, content) VALUES (?1, 'test', 'c')",
            [uid],
        )
        .unwrap();
        let prompt_id = conn.last_insert_rowid();

        tags::link_prompt_tag(conn, prompt_id, tag.id)?;
        tags::unlink_prompt_tag(conn, prompt_id, tag.id)?;

        let prompt_tags = tags::get_tags_for_prompt(conn, prompt_id)?;
        assert!(prompt_tags.is_empty());
        Ok(())
    })
    .unwrap();
}

#[test]
fn find_tag_by_name() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "findtaguser");
        tags::create_tag(conn, uid, "findable")?;

        let found = tags::find_tag_by_name(conn, uid, "findable")?;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "findable");

        let not_found = tags::find_tag_by_name(conn, uid, "nope")?;
        assert!(not_found.is_none());
        Ok(())
    })
    .unwrap();
}
