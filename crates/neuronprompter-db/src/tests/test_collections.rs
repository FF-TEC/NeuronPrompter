// =============================================================================
// Collection repository integration tests.
//
// Verifies CRUD, junction table operations, and uniqueness constraints.
// =============================================================================

use neuronprompter_core::domain::user::NewUser;

use super::setup_db;
use crate::ConnectionProvider;
use crate::repo::{collections, users};

fn create_user(conn: &rusqlite::Connection, username: &str) -> i64 {
    let new = NewUser {
        username: username.to_owned(),
        display_name: format!("Display {username}"),
    };
    users::create_user(conn, &new).expect("user creation").id
}

#[test]
fn create_and_list_collections() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "coluser");
        collections::create_collection(conn, uid, "work")?;
        collections::create_collection(conn, uid, "personal")?;

        let list = collections::list_collections_for_user(conn, uid)?;
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "personal");
        assert_eq!(list[1].name, "work");
        Ok(())
    })
    .unwrap();
}

#[test]
fn duplicate_collection_name_returns_error() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "dupcol");
        collections::create_collection(conn, uid, "same")?;
        let result = collections::create_collection(conn, uid, "same");
        assert!(result.is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn rename_collection() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "rencol");
        let col = collections::create_collection(conn, uid, "old_col")?;
        collections::rename_collection(conn, col.id, "new_col")?;

        let found = collections::find_collection_by_name(conn, uid, "new_col")?;
        assert!(found.is_some());
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_collection() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "delcol");
        let col = collections::create_collection(conn, uid, "temporary")?;
        collections::delete_collection(conn, col.id)?;

        let list = collections::list_collections_for_user(conn, uid)?;
        assert!(list.is_empty());
        Ok(())
    })
    .unwrap();
}

#[test]
fn link_and_get_collections_for_prompt() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "lnkcol");
        let col = collections::create_collection(conn, uid, "linked_col")?;

        conn.execute(
            "INSERT INTO prompts (user_id, title, content) VALUES (?1, 'test', 'c')",
            [uid],
        )
        .unwrap();
        let prompt_id = conn.last_insert_rowid();

        collections::link_prompt_collection(conn, prompt_id, col.id)?;

        let cols = collections::get_collections_for_prompt(conn, prompt_id)?;
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].name, "linked_col");
        Ok(())
    })
    .unwrap();
}

#[test]
fn unlink_collection_removes_junction() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "unlnkcol");
        let col = collections::create_collection(conn, uid, "unlink_col")?;

        conn.execute(
            "INSERT INTO prompts (user_id, title, content) VALUES (?1, 'test', 'c')",
            [uid],
        )
        .unwrap();
        let prompt_id = conn.last_insert_rowid();

        collections::link_prompt_collection(conn, prompt_id, col.id)?;
        collections::unlink_prompt_collection(conn, prompt_id, col.id)?;

        let cols = collections::get_collections_for_prompt(conn, prompt_id)?;
        assert!(cols.is_empty());
        Ok(())
    })
    .unwrap();
}
