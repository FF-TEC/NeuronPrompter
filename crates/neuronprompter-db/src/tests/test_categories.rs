// =============================================================================
// Category repository integration tests.
//
// Verifies CRUD, junction table operations, and uniqueness constraints.
// =============================================================================

use neuronprompter_core::domain::user::NewUser;

use super::setup_db;
use crate::ConnectionProvider;
use crate::repo::{categories, users};

fn create_user(conn: &rusqlite::Connection, username: &str) -> i64 {
    let new = NewUser {
        username: username.to_owned(),
        display_name: format!("Display {username}"),
    };
    users::create_user(conn, &new).expect("user creation").id
}

#[test]
fn create_and_list_categories() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "catuser");
        categories::create_category(conn, uid, "writing")?;
        categories::create_category(conn, uid, "coding")?;

        let list = categories::list_categories_for_user(conn, uid)?;
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "coding");
        assert_eq!(list[1].name, "writing");
        Ok(())
    })
    .unwrap();
}

#[test]
fn duplicate_category_name_returns_error() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "dupcat");
        categories::create_category(conn, uid, "same")?;
        let result = categories::create_category(conn, uid, "same");
        assert!(result.is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn rename_category() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "rencat");
        let cat = categories::create_category(conn, uid, "old_cat")?;
        categories::rename_category(conn, cat.id, "new_cat")?;

        let found = categories::find_category_by_name(conn, uid, "new_cat")?;
        assert!(found.is_some());
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_category() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "delcat");
        let cat = categories::create_category(conn, uid, "temporary")?;
        categories::delete_category(conn, cat.id)?;

        let list = categories::list_categories_for_user(conn, uid)?;
        assert!(list.is_empty());
        Ok(())
    })
    .unwrap();
}

#[test]
fn link_and_get_categories_for_prompt() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "lnkcat");
        let cat = categories::create_category(conn, uid, "linked_cat")?;

        conn.execute(
            "INSERT INTO prompts (user_id, title, content) VALUES (?1, 'test', 'c')",
            [uid],
        )
        .unwrap();
        let prompt_id = conn.last_insert_rowid();

        categories::link_prompt_category(conn, prompt_id, cat.id)?;

        let cats = categories::get_categories_for_prompt(conn, prompt_id)?;
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].name, "linked_cat");
        Ok(())
    })
    .unwrap();
}

#[test]
fn unlink_category_removes_junction() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "unlnkcat");
        let cat = categories::create_category(conn, uid, "unlink_cat")?;

        conn.execute(
            "INSERT INTO prompts (user_id, title, content) VALUES (?1, 'test', 'c')",
            [uid],
        )
        .unwrap();
        let prompt_id = conn.last_insert_rowid();

        categories::link_prompt_category(conn, prompt_id, cat.id)?;
        categories::unlink_prompt_category(conn, prompt_id, cat.id)?;

        let cats = categories::get_categories_for_prompt(conn, prompt_id)?;
        assert!(cats.is_empty());
        Ok(())
    })
    .unwrap();
}
