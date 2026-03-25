// =============================================================================
// Script repository integration tests.
//
// Verifies CRUD operations, association retrieval, filtering, updates,
// duplication, favorite/archive toggling, cascade deletion, and FTS5 search.
// =============================================================================

use neuronprompter_core::domain::script::{NewScript, ScriptFilter};
use neuronprompter_core::domain::user::NewUser;

use super::setup_db;
use crate::ConnectionProvider;
use crate::repo::{categories, collections, scripts, tags, users};

fn create_user(conn: &rusqlite::Connection, username: &str) -> i64 {
    let new = NewUser {
        username: username.to_owned(),
        display_name: format!("Display {username}"),
    };
    users::create_user(conn, &new).expect("user creation").id
}

fn make_script(user_id: i64, title: &str, lang: &str) -> NewScript {
    NewScript {
        user_id,
        title: title.to_owned(),
        content: format!("# Content for {title}"),
        script_language: lang.to_owned(),
        description: None,
        notes: None,
        language: None,
        source_path: None,
        is_synced: false,
        tag_ids: Vec::new(),
        category_ids: Vec::new(),
        collection_ids: Vec::new(),
    }
}

#[test]
fn create_and_get_script() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "scriptuser");
        let new = make_script(uid, "My Script", "python");
        let script = scripts::create_script(conn, &new)?;

        assert_eq!(script.title, "My Script");
        assert_eq!(script.user_id, uid);
        assert_eq!(script.script_language, "python");
        assert_eq!(script.current_version, 1);
        assert!(!script.is_favorite);
        assert!(!script.is_archived);

        let fetched = scripts::get_script(conn, script.id)?;
        assert_eq!(fetched.title, script.title);
        assert_eq!(fetched.script_language, "python");
        Ok(())
    })
    .unwrap();
}

#[test]
fn create_script_with_associations() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "assocuser");
        let tag = tags::create_tag(conn, uid, "test_tag")?;
        let cat = categories::create_category(conn, uid, "test_cat")?;
        let col = collections::create_collection(conn, uid, "test_col")?;

        let new = NewScript {
            tag_ids: vec![tag.id],
            category_ids: vec![cat.id],
            collection_ids: vec![col.id],
            ..make_script(uid, "Associated Script", "bash")
        };
        let script = scripts::create_script(conn, &new)?;

        let with_assoc = scripts::get_script_with_associations(conn, script.id)?;
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
fn get_script_with_associations() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "getassocuser");
        let tag = tags::create_tag(conn, uid, "resolve_tag")?;
        let cat = categories::create_category(conn, uid, "resolve_cat")?;
        let col = collections::create_collection(conn, uid, "resolve_col")?;

        let new = NewScript {
            tag_ids: vec![tag.id],
            category_ids: vec![cat.id],
            collection_ids: vec![col.id],
            ..make_script(uid, "Resolve Test", "javascript")
        };
        let script = scripts::create_script(conn, &new)?;

        let swa = scripts::get_script_with_associations(conn, script.id)?;
        assert_eq!(swa.script.title, "Resolve Test");
        assert_eq!(swa.script.script_language, "javascript");
        assert_eq!(swa.tags.len(), 1);
        assert_eq!(swa.tags[0].id, tag.id);
        assert_eq!(swa.categories.len(), 1);
        assert_eq!(swa.categories[0].id, cat.id);
        assert_eq!(swa.collections.len(), 1);
        assert_eq!(swa.collections[0].id, col.id);
        Ok(())
    })
    .unwrap();
}

#[test]
fn list_scripts_filter_by_user() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid1 = create_user(conn, "listuser_a");
        let uid2 = create_user(conn, "listuser_b");

        scripts::create_script(conn, &make_script(uid1, "User1 Script", "python"))?;
        scripts::create_script(conn, &make_script(uid2, "User2 Script", "bash"))?;

        let filter = ScriptFilter {
            user_id: Some(uid1),
            ..ScriptFilter::default()
        };
        let list = scripts::list_scripts(conn, &filter)?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "User1 Script");
        Ok(())
    })
    .unwrap();
}

#[test]
fn list_scripts_filter_by_favorite() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "favfilter");
        let s1 = scripts::create_script(conn, &make_script(uid, "Faved", "python"))?;
        scripts::create_script(conn, &make_script(uid, "Not Faved", "python"))?;
        scripts::set_favorite(conn, s1.id, true)?;

        let filter = ScriptFilter {
            is_favorite: Some(true),
            ..ScriptFilter::default()
        };
        let list = scripts::list_scripts(conn, &filter)?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Faved");
        Ok(())
    })
    .unwrap();
}

#[test]
fn list_scripts_filter_by_archive() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "archfilter");
        let s1 = scripts::create_script(conn, &make_script(uid, "Archived", "bash"))?;
        scripts::create_script(conn, &make_script(uid, "Active", "bash"))?;
        scripts::set_archived(conn, s1.id, true)?;

        let filter = ScriptFilter {
            is_archived: Some(true),
            ..ScriptFilter::default()
        };
        let list = scripts::list_scripts(conn, &filter)?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Archived");
        Ok(())
    })
    .unwrap();
}

#[test]
fn list_scripts_filter_by_tag() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "tagfilter");
        let tag = tags::create_tag(conn, uid, "filtered_tag")?;

        let s1 = scripts::create_script(
            conn,
            &NewScript {
                tag_ids: vec![tag.id],
                ..make_script(uid, "Tagged", "python")
            },
        )?;
        scripts::create_script(conn, &make_script(uid, "Untagged", "python"))?;

        let filter = ScriptFilter {
            tag_id: Some(tag.id),
            ..ScriptFilter::default()
        };
        let list = scripts::list_scripts(conn, &filter)?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, s1.id);
        Ok(())
    })
    .unwrap();
}

#[test]
fn list_scripts_filter_by_category() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "catfilter");
        let cat = categories::create_category(conn, uid, "filtered_cat")?;

        let s1 = scripts::create_script(
            conn,
            &NewScript {
                category_ids: vec![cat.id],
                ..make_script(uid, "Categorized", "bash")
            },
        )?;
        scripts::create_script(conn, &make_script(uid, "Uncategorized", "bash"))?;

        let filter = ScriptFilter {
            category_id: Some(cat.id),
            ..ScriptFilter::default()
        };
        let list = scripts::list_scripts(conn, &filter)?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, s1.id);
        Ok(())
    })
    .unwrap();
}

#[test]
fn list_scripts_filter_by_collection() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "colfilter");
        let col = collections::create_collection(conn, uid, "filtered_col")?;

        let s1 = scripts::create_script(
            conn,
            &NewScript {
                collection_ids: vec![col.id],
                ..make_script(uid, "Collected", "python")
            },
        )?;
        scripts::create_script(conn, &make_script(uid, "Loose", "python"))?;

        let filter = ScriptFilter {
            collection_id: Some(col.id),
            ..ScriptFilter::default()
        };
        let list = scripts::list_scripts(conn, &filter)?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, s1.id);
        Ok(())
    })
    .unwrap();
}

#[test]
fn update_script_fields() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "updateuser");
        let script = scripts::create_script(conn, &make_script(uid, "Original", "python"))?;
        assert_eq!(script.current_version, 1);

        let updated = scripts::update_script_fields(
            conn,
            script.id,
            Some("Updated Title"),
            Some("Updated content"),
            None,
            None,
            None,
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
fn update_script_language() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "langupdate");
        let script = scripts::create_script(conn, &make_script(uid, "Lang Test", "python"))?;
        assert_eq!(script.script_language, "python");

        let updated = scripts::update_script_fields(
            conn,
            script.id,
            None,
            None,
            None,
            None,
            Some("rust"),
            None,
            None,
            None,
            None,
        )?;

        assert_eq!(updated.script_language, "rust");
        assert_eq!(updated.current_version, 2);
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_script() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "deluser");
        let script = scripts::create_script(conn, &make_script(uid, "To Delete", "bash"))?;
        scripts::delete_script(conn, script.id)?;

        let result = scripts::get_script(conn, script.id);
        assert!(result.is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_nonexistent_script() {
    let db = setup_db();
    db.with_connection(|conn| {
        let result = scripts::delete_script(conn, 9999);
        assert!(result.is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn set_favorite_and_archived() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "toggleuser");
        let script = scripts::create_script(conn, &make_script(uid, "Toggle Test", "python"))?;
        assert!(!script.is_favorite);
        assert!(!script.is_archived);

        // Toggle favorite on.
        scripts::set_favorite(conn, script.id, true)?;
        let fetched = scripts::get_script(conn, script.id)?;
        assert!(fetched.is_favorite);

        // Toggle favorite off.
        scripts::set_favorite(conn, script.id, false)?;
        let fetched = scripts::get_script(conn, script.id)?;
        assert!(!fetched.is_favorite);

        // Toggle archived on.
        scripts::set_archived(conn, script.id, true)?;
        let fetched = scripts::get_script(conn, script.id)?;
        assert!(fetched.is_archived);

        // Toggle archived off.
        scripts::set_archived(conn, script.id, false)?;
        let fetched = scripts::get_script(conn, script.id)?;
        assert!(!fetched.is_archived);
        Ok(())
    })
    .unwrap();
}

#[test]
fn duplicate_script() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "dupuser");
        let tag = tags::create_tag(conn, uid, "dup_tag")?;

        let new = NewScript {
            tag_ids: vec![tag.id],
            ..make_script(uid, "Original Script", "python")
        };
        let original = scripts::create_script(conn, &new)?;

        let dup = scripts::duplicate_script(conn, original.id)?;
        assert_eq!(dup.title, "Original Script (copy)");
        assert_eq!(dup.content, original.content);
        assert_eq!(dup.script_language, "python");
        assert_eq!(dup.current_version, 1);
        assert_ne!(dup.id, original.id);

        // Verify associations were copied.
        let dup_tags = scripts::get_tags_for_script(conn, dup.id)?;
        assert_eq!(dup_tags.len(), 1);
        assert_eq!(dup_tags[0].name, "dup_tag");
        Ok(())
    })
    .unwrap();
}

#[test]
fn search_scripts_fts() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "searchuser");

        scripts::create_script(
            conn,
            &NewScript {
                title: "Machine Learning Pipeline".to_owned(),
                content: "import tensorflow as tf".to_owned(),
                ..make_script(uid, "", "python")
            },
        )?;
        scripts::create_script(
            conn,
            &NewScript {
                title: "Web Server Setup".to_owned(),
                content: "#!/bin/bash\nnginx -s reload".to_owned(),
                ..make_script(uid, "", "bash")
            },
        )?;

        let results = scripts::search_scripts(conn, uid, "machine", &ScriptFilter::default())?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Machine Learning Pipeline");

        // Search by content.
        let results2 = scripts::search_scripts(conn, uid, "tensorflow", &ScriptFilter::default())?;
        assert_eq!(results2.len(), 1);
        assert_eq!(results2[0].title, "Machine Learning Pipeline");
        Ok(())
    })
    .unwrap();
}
