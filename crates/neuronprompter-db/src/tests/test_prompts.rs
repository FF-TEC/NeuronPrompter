// =============================================================================
// Prompt repository integration tests.
//
// Verifies CRUD operations, association retrieval, filtering, updates,
// duplication, favorite/archive toggling, and cascade deletion.
// =============================================================================

use neuronprompter_core::domain::prompt::{NewPrompt, PromptFilter};
use neuronprompter_core::domain::user::NewUser;

use super::setup_db;
use crate::ConnectionProvider;
use crate::repo::{categories, collections, prompts, tags, users};

fn create_user(conn: &rusqlite::Connection, username: &str) -> i64 {
    let new = NewUser {
        username: username.to_owned(),
        display_name: format!("Display {username}"),
    };
    users::create_user(conn, &new).expect("user creation").id
}

fn make_prompt(user_id: i64, title: &str) -> NewPrompt {
    NewPrompt {
        user_id,
        title: title.to_owned(),
        content: format!("Content for {title}"),
        description: None,
        notes: None,
        language: None,
        tag_ids: Vec::new(),
        category_ids: Vec::new(),
        collection_ids: Vec::new(),
    }
}

#[test]
fn create_and_get_prompt() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "promptuser");
        let new = make_prompt(uid, "My Prompt");
        let prompt = prompts::create_prompt(conn, &new)?;

        assert_eq!(prompt.title, "My Prompt");
        assert_eq!(prompt.user_id, uid);
        assert_eq!(prompt.current_version, 1);
        assert!(!prompt.is_favorite);
        assert!(!prompt.is_archived);

        let fetched = prompts::get_prompt(conn, prompt.id)?;
        assert_eq!(fetched.title, prompt.title);
        Ok(())
    })
    .unwrap();
}

#[test]
fn create_prompt_with_associations() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "assocuser");
        let tag = tags::create_tag(conn, uid, "test_tag")?;
        let cat = categories::create_category(conn, uid, "test_cat")?;
        let col = collections::create_collection(conn, uid, "test_col")?;

        let new = NewPrompt {
            tag_ids: vec![tag.id],
            category_ids: vec![cat.id],
            collection_ids: vec![col.id],
            ..make_prompt(uid, "Associated Prompt")
        };
        let prompt = prompts::create_prompt(conn, &new)?;

        let with_assoc = prompts::get_prompt_with_associations(conn, prompt.id)?;
        assert_eq!(with_assoc.tags.len(), 1);
        assert_eq!(with_assoc.tags[0].name, "test_tag");
        assert_eq!(with_assoc.categories.len(), 1);
        assert_eq!(with_assoc.categories[0].name, "test_cat");
        assert_eq!(with_assoc.collections.len(), 1);
        assert_eq!(with_assoc.collections[0].name, "test_col");
        Ok(())
    })
    .unwrap();
}

#[test]
fn list_prompts_with_user_filter() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid1 = create_user(conn, "listuser_a");
        let uid2 = create_user(conn, "listuser_b");

        prompts::create_prompt(conn, &make_prompt(uid1, "User1 Prompt"))?;
        prompts::create_prompt(conn, &make_prompt(uid2, "User2 Prompt"))?;

        let filter = PromptFilter {
            user_id: Some(uid1),
            ..PromptFilter::default()
        };
        let list = prompts::list_prompts(conn, &filter)?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "User1 Prompt");
        Ok(())
    })
    .unwrap();
}

#[test]
fn list_prompts_with_tag_filter() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "tagfilter");
        let tag = tags::create_tag(conn, uid, "filtered_tag")?;

        let p1 = prompts::create_prompt(
            conn,
            &NewPrompt {
                tag_ids: vec![tag.id],
                ..make_prompt(uid, "Tagged")
            },
        )?;
        prompts::create_prompt(conn, &make_prompt(uid, "Untagged"))?;

        let filter = PromptFilter {
            tag_id: Some(tag.id),
            ..PromptFilter::default()
        };
        let list = prompts::list_prompts(conn, &filter)?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, p1.id);
        Ok(())
    })
    .unwrap();
}

#[test]
fn update_prompt_fields() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "updateuser");
        let prompt = prompts::create_prompt(conn, &make_prompt(uid, "Original"))?;
        assert_eq!(prompt.current_version, 1);

        let updated = prompts::update_prompt_fields(
            conn,
            prompt.id,
            Some("Updated Title"),
            Some("Updated content"),
            None,
            None,
            None,
            None,
        )?;

        assert_eq!(updated.title, "Updated Title");
        assert_eq!(updated.content, "Updated content");
        assert_eq!(updated.current_version, 2);
        Ok(())
    })
    .unwrap();
}

#[test]
fn update_nonexistent_prompt_returns_not_found() {
    let db = setup_db();
    db.with_connection(|conn| {
        let result =
            prompts::update_prompt_fields(conn, 9999, Some("Title"), None, None, None, None, None);
        assert!(result.is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_prompt() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "deluser");
        let prompt = prompts::create_prompt(conn, &make_prompt(uid, "To Delete"))?;
        prompts::delete_prompt(conn, prompt.id)?;

        let result = prompts::get_prompt(conn, prompt.id);
        assert!(result.is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn set_favorite() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "favuser");
        let prompt = prompts::create_prompt(conn, &make_prompt(uid, "Fav Test"))?;
        assert!(!prompt.is_favorite);

        prompts::set_favorite(conn, prompt.id, true)?;
        let fetched = prompts::get_prompt(conn, prompt.id)?;
        assert!(fetched.is_favorite);

        prompts::set_favorite(conn, prompt.id, false)?;
        let fetched2 = prompts::get_prompt(conn, prompt.id)?;
        assert!(!fetched2.is_favorite);
        Ok(())
    })
    .unwrap();
}

#[test]
fn set_archived() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "archuser");
        let prompt = prompts::create_prompt(conn, &make_prompt(uid, "Arch Test"))?;
        assert!(!prompt.is_archived);

        prompts::set_archived(conn, prompt.id, true)?;
        let fetched = prompts::get_prompt(conn, prompt.id)?;
        assert!(fetched.is_archived);
        Ok(())
    })
    .unwrap();
}

#[test]
fn list_prompts_favorite_filter() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "favfilter");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "Faved"))?;
        prompts::create_prompt(conn, &make_prompt(uid, "Not Faved"))?;
        prompts::set_favorite(conn, p1.id, true)?;

        let filter = PromptFilter {
            is_favorite: Some(true),
            ..PromptFilter::default()
        };
        let list = prompts::list_prompts(conn, &filter)?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Faved");
        Ok(())
    })
    .unwrap();
}

#[test]
fn duplicate_prompt_copies_data_and_associations() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "dupuser");
        let tag = tags::create_tag(conn, uid, "dup_tag")?;

        let new = NewPrompt {
            tag_ids: vec![tag.id],
            ..make_prompt(uid, "Original Prompt")
        };
        let original = prompts::create_prompt(conn, &new)?;

        let dup = prompts::duplicate_prompt(conn, original.id)?;
        assert_eq!(dup.title, "Original Prompt (copy)");
        assert_eq!(dup.content, original.content);
        assert_eq!(dup.current_version, 1);
        assert_ne!(dup.id, original.id);

        // Verify associations were copied.
        let dup_tags = tags::get_tags_for_prompt(conn, dup.id)?;
        assert_eq!(dup_tags.len(), 1);
        assert_eq!(dup_tags[0].name, "dup_tag");
        Ok(())
    })
    .unwrap();
}

#[test]
fn cascade_delete_user_removes_prompts_and_associations() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "cascade_user");
        let tag = tags::create_tag(conn, uid, "cascade_tag")?;
        let cat = categories::create_category(conn, uid, "cascade_cat")?;

        let new = NewPrompt {
            tag_ids: vec![tag.id],
            category_ids: vec![cat.id],
            ..make_prompt(uid, "Cascade Prompt")
        };
        let prompt = prompts::create_prompt(conn, &new)?;

        // Insert a version snapshot.
        conn.execute(
            "INSERT INTO prompt_versions (prompt_id, version_number, title, content) \
             VALUES (?1, 1, 'v1', 'c1')",
            [prompt.id],
        )
        .unwrap();

        // Delete user; all dependent data should be removed via CASCADE.
        users::delete_user(conn, uid)?;

        // Verify everything is gone.
        let prompt_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM prompts WHERE user_id = ?1",
                [uid],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(prompt_count, 0, "prompts should be deleted");

        let tag_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tags WHERE user_id = ?1",
                [uid],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tag_count, 0, "tags should be deleted");

        let junction_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM prompt_tags", [], |row| row.get(0))
            .unwrap();
        assert_eq!(junction_count, 0, "junction rows should be deleted");

        let version_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM prompt_versions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version_count, 0, "version rows should be deleted");
        Ok(())
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// Bug 5 – Archive / Favorite toggle payload
//
// Verifies that toggling archive and favorite flags via the repo layer
// correctly persists the new state and that each flag is independent.
// ---------------------------------------------------------------------------

#[test]
fn toggle_archive_true_then_verify_bug5() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "arch_toggle");
        let prompt = prompts::create_prompt(conn, &make_prompt(uid, "Archive Toggle"))?;
        assert!(
            !prompt.is_archived,
            "newly created prompt must not be archived"
        );

        prompts::set_archived(conn, prompt.id, true)?;
        let fetched = prompts::get_prompt(conn, prompt.id)?;
        assert!(
            fetched.is_archived,
            "prompt must be archived after toggling to true"
        );
        // Favorite should remain untouched.
        assert!(
            !fetched.is_favorite,
            "toggling archive must not affect favorite flag"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn toggle_favorite_true_then_verify_bug5() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "fav_toggle");
        let prompt = prompts::create_prompt(conn, &make_prompt(uid, "Favorite Toggle"))?;
        assert!(
            !prompt.is_favorite,
            "newly created prompt must not be favorited"
        );

        prompts::set_favorite(conn, prompt.id, true)?;
        let fetched = prompts::get_prompt(conn, prompt.id)?;
        assert!(
            fetched.is_favorite,
            "prompt must be favorited after toggling to true"
        );
        // Archived should remain untouched.
        assert!(
            !fetched.is_archived,
            "toggling favorite must not affect archive flag"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn toggle_archive_and_favorite_independently_bug5() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "indep_toggle");
        let prompt = prompts::create_prompt(conn, &make_prompt(uid, "Independent Toggle"))?;

        // Set both flags to true independently.
        prompts::set_archived(conn, prompt.id, true)?;
        prompts::set_favorite(conn, prompt.id, true)?;
        let fetched = prompts::get_prompt(conn, prompt.id)?;
        assert!(fetched.is_archived, "archived must be true");
        assert!(fetched.is_favorite, "favorite must be true");

        // Toggle archive off; favorite must remain true.
        prompts::set_archived(conn, prompt.id, false)?;
        let fetched = prompts::get_prompt(conn, prompt.id)?;
        assert!(
            !fetched.is_archived,
            "archived must be false after toggle off"
        );
        assert!(
            fetched.is_favorite,
            "favorite must remain true when archive is toggled off"
        );

        // Toggle favorite off; archive must remain false.
        prompts::set_favorite(conn, prompt.id, false)?;
        let fetched = prompts::get_prompt(conn, prompt.id)?;
        assert!(
            !fetched.is_favorite,
            "favorite must be false after toggle off"
        );
        assert!(!fetched.is_archived, "archived must remain false");
        Ok(())
    })
    .unwrap();
}

#[test]
fn get_nonexistent_prompt_returns_not_found() {
    let db = setup_db();
    db.with_connection(|conn| {
        let result = prompts::get_prompt(conn, 9999);
        assert!(result.is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn sql_injection_payloads_stored_verbatim() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "injection_user");

        let malicious_title = "'; DROP TABLE prompts; --";
        let malicious_content = "\" OR 1=1 --";
        let new = NewPrompt {
            user_id: uid,
            title: malicious_title.to_owned(),
            content: malicious_content.to_owned(),
            description: None,
            notes: None,
            language: None,
            tag_ids: Vec::new(),
            category_ids: Vec::new(),
            collection_ids: Vec::new(),
        };
        let prompt = prompts::create_prompt(conn, &new)?;

        let fetched = prompts::get_prompt(conn, prompt.id)?;
        assert_eq!(fetched.title, malicious_title);
        assert_eq!(fetched.content, malicious_content);

        // Verify the prompts table still exists (injection did not execute).
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM prompts", [], |row| row.get(0))
            .unwrap();
        assert!(count >= 1);
        Ok(())
    })
    .unwrap();
}
