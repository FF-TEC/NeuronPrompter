// =============================================================================
// User repository integration tests.
//
// Verifies CRUD operations against an in-memory SQLite database.
// =============================================================================

use neuronprompter_core::domain::user::NewUser;

use super::setup_db;
use crate::ConnectionProvider;
use crate::repo::users;

/// Helper: creates a user with the given username and returns the persisted
/// entity.
fn create_test_user(
    conn: &rusqlite::Connection,
    username: &str,
) -> neuronprompter_core::domain::user::User {
    let new = NewUser {
        username: username.to_owned(),
        display_name: format!("Display {username}"),
    };
    users::create_user(conn, &new).expect("user creation should succeed")
}

#[test]
fn create_and_get_user() {
    let db = setup_db();
    db.with_connection(|conn| {
        let user = create_test_user(conn, "alice");
        assert_eq!(user.username, "alice");
        assert_eq!(user.display_name, "Display alice");
        assert!(user.id > 0);

        let fetched = users::get_user(conn, user.id)?;
        assert_eq!(fetched.username, user.username);
        assert_eq!(fetched.display_name, user.display_name);
        Ok(())
    })
    .unwrap();
}

#[test]
fn list_users_returns_all_ordered_by_username() {
    let db = setup_db();
    db.with_connection(|conn| {
        create_test_user(conn, "charlie");
        create_test_user(conn, "alice");
        create_test_user(conn, "bob");

        let list = users::list_users(conn)?;
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].username, "alice");
        assert_eq!(list[1].username, "bob");
        assert_eq!(list[2].username, "charlie");
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_user_removes_record() {
    let db = setup_db();
    db.with_connection(|conn| {
        let user = create_test_user(conn, "to_delete");
        users::delete_user(conn, user.id)?;

        let result = users::get_user(conn, user.id);
        assert!(result.is_err(), "deleted user should not be found");
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_nonexistent_user_returns_not_found() {
    let db = setup_db();
    db.with_connection(|conn| {
        let result = users::delete_user(conn, 9999);
        assert!(result.is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn find_user_by_username_returns_some() {
    let db = setup_db();
    db.with_connection(|conn| {
        create_test_user(conn, "findme");

        let found = users::find_user_by_username(conn, "findme")?;
        assert!(found.is_some());
        assert_eq!(found.unwrap().username, "findme");
        Ok(())
    })
    .unwrap();
}

#[test]
fn find_user_by_username_returns_none_for_missing() {
    let db = setup_db();
    db.with_connection(|conn| {
        let found = users::find_user_by_username(conn, "nonexistent")?;
        assert!(found.is_none());
        Ok(())
    })
    .unwrap();
}

#[test]
fn get_nonexistent_user_returns_not_found() {
    let db = setup_db();
    db.with_connection(|conn| {
        let result = users::get_user(conn, 999);
        assert!(result.is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn duplicate_username_returns_duplicate_error() {
    let db = setup_db();
    db.with_connection(|conn| {
        create_test_user(conn, "alice");
        let dup = NewUser {
            username: "alice".to_owned(),
            display_name: "Another Alice".to_owned(),
        };
        let result = users::create_user(conn, &dup);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(
                err,
                crate::DbError::Core(neuronprompter_core::CoreError::Duplicate { .. })
            ),
            "expected Duplicate error, got: {err:?}"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn same_display_name_different_username_succeeds() {
    let db = setup_db();
    db.with_connection(|conn| {
        let u1 = NewUser {
            username: "alice".to_owned(),
            display_name: "Same Name".to_owned(),
        };
        let u2 = NewUser {
            username: "bob".to_owned(),
            display_name: "Same Name".to_owned(),
        };
        users::create_user(conn, &u1)?;
        users::create_user(conn, &u2)?;
        let list = users::list_users(conn)?;
        assert_eq!(list.len(), 2);
        Ok(())
    })
    .unwrap();
}
