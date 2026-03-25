// =============================================================================
// Version repository integration tests.
//
// Verifies version snapshot insertion, listing, and retrieval by id and
// by version number.
// =============================================================================

use neuronprompter_core::domain::user::NewUser;

use super::setup_db;
use crate::ConnectionProvider;
use crate::repo::{users, versions};

fn create_user(conn: &rusqlite::Connection) -> i64 {
    let new = NewUser {
        username: "versionuser".to_owned(),
        display_name: "Version User".to_owned(),
    };
    users::create_user(conn, &new).expect("user creation").id
}

fn create_prompt(conn: &rusqlite::Connection, uid: i64) -> i64 {
    conn.execute(
        "INSERT INTO prompts (user_id, title, content) VALUES (?1, 'Version Test', 'body')",
        [uid],
    )
    .unwrap();
    conn.last_insert_rowid()
}

#[test]
fn insert_and_list_versions() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn);
        let pid = create_prompt(conn, uid);

        versions::insert_version(conn, pid, 1, "V1 Title", "V1 content", None, None, None)?;
        versions::insert_version(
            conn,
            pid,
            2,
            "V2 Title",
            "V2 content",
            Some("desc"),
            None,
            Some("en"),
        )?;

        let list = versions::list_versions_for_prompt(conn, pid)?;
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
fn get_version_by_id() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn);
        let pid = create_prompt(conn, uid);

        let v = versions::insert_version(conn, pid, 1, "Title", "Content", None, None, None)?;

        let fetched = versions::get_version_by_id(conn, v.id)?;
        assert_eq!(fetched.title, "Title");
        assert_eq!(fetched.content, "Content");
        Ok(())
    })
    .unwrap();
}

#[test]
fn get_version_by_number() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn);
        let pid = create_prompt(conn, uid);

        versions::insert_version(conn, pid, 1, "First", "c1", None, None, None)?;
        versions::insert_version(conn, pid, 2, "Second", "c2", None, None, None)?;

        let v2 = versions::get_version_by_number(conn, pid, 2)?;
        assert_eq!(v2.title, "Second");
        assert_eq!(v2.content, "c2");
        Ok(())
    })
    .unwrap();
}

#[test]
fn get_nonexistent_version_returns_error() {
    let db = setup_db();
    db.with_connection(|conn| {
        let result = versions::get_version_by_id(conn, 9999);
        assert!(result.is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn version_preserves_optional_fields() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn);
        let pid = create_prompt(conn, uid);

        let v = versions::insert_version(
            conn,
            pid,
            1,
            "Full",
            "body",
            Some("description"),
            Some("notes text"),
            Some("de"),
        )?;

        assert_eq!(v.description.as_deref(), Some("description"));
        assert_eq!(v.notes.as_deref(), Some("notes text"));
        assert_eq!(v.language.as_deref(), Some("de"));
        Ok(())
    })
    .unwrap();
}
