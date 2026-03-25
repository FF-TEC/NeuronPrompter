// =============================================================================
// User service integration tests.
//
// Verifies user creation with default settings, listing, switching,
// deletion, and validation error propagation.
// =============================================================================

use neuronprompter_core::domain::user::NewUser;

use super::{create_test_user, setup_db};
use crate::user_service;
use neuronprompter_db::ConnectionProvider;

#[test]
fn create_user_inserts_default_settings() {
    let db = setup_db();
    let uid = create_test_user(&db, "testuser");

    // Verify the user exists via the service layer.
    let users = user_service::list_users(&db).unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].id, uid);

    // Verify default user_settings row was inserted alongside the user.
    db.with_connection(|conn| {
        let settings = neuronprompter_db::repo::settings::get_user_settings(conn, uid)?;
        assert_eq!(settings.ollama_base_url, "http://localhost:11434");
        Ok(())
    })
    .unwrap();
}

#[test]
fn create_user_with_invalid_username_returns_validation_error() {
    let db = setup_db();
    let new = NewUser {
        username: "Invalid User".to_owned(),
        display_name: "Display".to_owned(),
    };
    let result = user_service::create_user(&db, &new);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("username"),
        "error should mention username field"
    );
}

#[test]
fn switch_user_stores_last_user_id() {
    let db = setup_db();
    let uid1 = create_test_user(&db, "user_one");
    let uid2 = create_test_user(&db, "user_two");

    user_service::switch_user(&db, uid2).unwrap();

    db.with_connection(|conn| {
        let setting = neuronprompter_db::repo::settings::get_app_setting(conn, "last_user_id")?;
        assert_eq!(setting.map(|s| s.value), Some(uid2.to_string()));
        Ok(())
    })
    .unwrap();

    // Switch back to user 1 to confirm the setting updates.
    user_service::switch_user(&db, uid1).unwrap();
    db.with_connection(|conn| {
        let setting = neuronprompter_db::repo::settings::get_app_setting(conn, "last_user_id")?;
        assert_eq!(setting.map(|s| s.value), Some(uid1.to_string()));
        Ok(())
    })
    .unwrap();
}

#[test]
fn switch_to_nonexistent_user_fails() {
    let db = setup_db();
    let result = user_service::switch_user(&db, 99999);
    assert!(result.is_err());
}

#[test]
fn delete_user_cascades_all_data() {
    let db = setup_db();
    let uid = create_test_user(&db, "deleteuser");

    // Create some owned data.
    let prompt =
        crate::prompt_service::create_prompt(&db, &super::make_prompt(uid, "Test")).unwrap();
    crate::tag_service::create_tag(&db, uid, "my_tag").unwrap();

    // Delete the user.
    user_service::delete_user(&db, uid).unwrap();

    // User should be gone.
    let users = user_service::list_users(&db).unwrap();
    assert!(users.is_empty());

    // Prompt should be gone (cascade).
    let result = crate::prompt_service::get_prompt(&db, prompt.id);
    assert!(result.is_err());
}
