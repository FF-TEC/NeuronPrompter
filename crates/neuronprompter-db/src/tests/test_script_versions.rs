// =============================================================================
// Script version repository integration tests.
//
// Verifies version snapshot insertion, listing, retrieval by id and by
// version number, and script_language preservation.
// =============================================================================

use neuronprompter_core::domain::script::NewScript;
use neuronprompter_core::domain::user::NewUser;

use super::setup_db;
use crate::ConnectionProvider;
use crate::repo::{script_versions, scripts, users};

fn create_user(conn: &rusqlite::Connection) -> i64 {
    let new = NewUser {
        username: "scriptversionuser".to_owned(),
        display_name: "Script Version User".to_owned(),
    };
    users::create_user(conn, &new).expect("user creation").id
}

fn create_script(conn: &rusqlite::Connection, uid: i64) -> i64 {
    let new = NewScript {
        user_id: uid,
        title: "Version Test".to_owned(),
        content: "body".to_owned(),
        script_language: "python".to_owned(),
        description: None,
        notes: None,
        language: None,
        source_path: None,
        is_synced: false,
        tag_ids: Vec::new(),
        category_ids: Vec::new(),
        collection_ids: Vec::new(),
    };
    scripts::create_script(conn, &new)
        .expect("script creation")
        .id
}

#[test]
fn insert_and_get_script_version() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn);
        let sid = create_script(conn, uid);

        let v = script_versions::insert_version(
            conn,
            sid,
            1,
            "V1 Title",
            "V1 content",
            None,
            None,
            "python",
            None,
        )?;

        assert_eq!(v.script_id, sid);
        assert_eq!(v.version_number, 1);
        assert_eq!(v.title, "V1 Title");
        assert_eq!(v.content, "V1 content");
        assert_eq!(v.script_language, "python");

        let fetched = script_versions::get_version_by_id(conn, v.id)?;
        assert_eq!(fetched.title, "V1 Title");
        assert_eq!(fetched.content, "V1 content");
        Ok(())
    })
    .unwrap();
}

#[test]
fn list_versions_for_script() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn);
        let sid = create_script(conn, uid);

        script_versions::insert_version(
            conn,
            sid,
            1,
            "V1 Title",
            "V1 content",
            None,
            None,
            "python",
            None,
        )?;
        script_versions::insert_version(
            conn,
            sid,
            2,
            "V2 Title",
            "V2 content",
            Some("desc"),
            None,
            "bash",
            Some("en"),
        )?;

        let list = script_versions::list_versions_for_script(conn, sid)?;
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].version_number, 1);
        assert_eq!(list[0].title, "V1 Title");
        assert_eq!(list[1].version_number, 2);
        assert_eq!(list[1].title, "V2 Title");
        assert_eq!(list[1].language.as_deref(), Some("en"));
        Ok(())
    })
    .unwrap();
}

#[test]
fn get_version_by_number() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn);
        let sid = create_script(conn, uid);

        script_versions::insert_version(conn, sid, 1, "First", "c1", None, None, "python", None)?;
        script_versions::insert_version(conn, sid, 2, "Second", "c2", None, None, "bash", None)?;

        let v2 = script_versions::get_version_by_number(conn, sid, 2)?;
        assert_eq!(v2.title, "Second");
        assert_eq!(v2.content, "c2");
        assert_eq!(v2.script_language, "bash");
        Ok(())
    })
    .unwrap();
}

#[test]
fn version_stores_script_language() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn);
        let sid = create_script(conn, uid);

        let v = script_versions::insert_version(
            conn,
            sid,
            1,
            "Full",
            "body",
            Some("description"),
            Some("notes text"),
            "javascript",
            Some("de"),
        )?;

        assert_eq!(v.script_language, "javascript");
        assert_eq!(v.description.as_deref(), Some("description"));
        assert_eq!(v.notes.as_deref(), Some("notes text"));
        assert_eq!(v.language.as_deref(), Some("de"));
        Ok(())
    })
    .unwrap();
}

#[test]
fn get_nonexistent_version_returns_error() {
    let db = setup_db();
    db.with_connection(|conn| {
        let result = script_versions::get_version_by_id(conn, 9999);
        assert!(result.is_err());
        Ok(())
    })
    .unwrap();
}
