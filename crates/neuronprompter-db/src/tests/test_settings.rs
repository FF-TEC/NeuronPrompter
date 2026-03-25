// =============================================================================
// Settings repository integration tests.
//
// Verifies app_settings key-value store and user_settings CRUD with enum
// serialization/deserialization round-trips.
// =============================================================================

use neuronprompter_core::domain::settings::{SortDirection, SortField, Theme, UserSettings};
use neuronprompter_core::domain::user::NewUser;

use super::setup_db;
use crate::ConnectionProvider;
use crate::repo::{settings, users};

fn create_user(conn: &rusqlite::Connection, username: &str) -> i64 {
    let new = NewUser {
        username: username.to_owned(),
        display_name: format!("Display {username}"),
    };
    users::create_user(conn, &new).expect("user creation").id
}

#[test]
fn app_setting_get_set_round_trip() {
    let db = setup_db();
    db.with_connection(|conn| {
        settings::set_app_setting(conn, "last_user_id", "42")?;

        let result = settings::get_app_setting(conn, "last_user_id")?;
        assert!(result.is_some());
        let setting = result.unwrap();
        assert_eq!(setting.key, "last_user_id");
        assert_eq!(setting.value, "42");
        Ok(())
    })
    .unwrap();
}

#[test]
fn app_setting_upsert_overwrites() {
    let db = setup_db();
    db.with_connection(|conn| {
        settings::set_app_setting(conn, "key", "original")?;
        settings::set_app_setting(conn, "key", "updated")?;

        let result = settings::get_app_setting(conn, "key")?;
        assert_eq!(result.unwrap().value, "updated");
        Ok(())
    })
    .unwrap();
}

#[test]
fn app_setting_get_missing_returns_none() {
    let db = setup_db();
    db.with_connection(|conn| {
        let result = settings::get_app_setting(conn, "nonexistent")?;
        assert!(result.is_none());
        Ok(())
    })
    .unwrap();
}

#[test]
fn create_default_user_settings_and_get() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "settings_user");
        settings::create_default_user_settings(conn, uid)?;

        let us = settings::get_user_settings(conn, uid)?;
        assert_eq!(us.user_id, uid);
        assert!(matches!(us.theme, Theme::Dark));
        assert!(matches!(us.sort_field, SortField::UpdatedAt));
        assert!(matches!(us.sort_direction, SortDirection::Desc));
        assert_eq!(us.ollama_base_url, "http://localhost:11434");
        assert!(!us.sidebar_collapsed);
        Ok(())
    })
    .unwrap();
}

#[test]
fn upsert_user_settings_creates_and_updates() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "upsert_user");

        // Create a collection so the FK on last_collection_id is satisfied.
        let col = crate::repo::collections::create_collection(conn, uid, "test col")?;

        let us = UserSettings {
            user_id: uid,
            theme: Theme::Dark,
            last_collection_id: Some(col.id),
            sidebar_collapsed: true,
            sort_field: SortField::Title,
            sort_direction: SortDirection::Asc,
            ollama_base_url: "http://custom:1234".to_owned(),
            ollama_model: Some("llama3".to_owned()),
            extra: "{}".to_owned(),
        };
        settings::upsert_user_settings(conn, &us)?;

        let fetched = settings::get_user_settings(conn, uid)?;
        assert!(matches!(fetched.theme, Theme::Dark));
        assert_eq!(fetched.last_collection_id, Some(col.id));
        assert!(fetched.sidebar_collapsed);
        assert!(matches!(fetched.sort_field, SortField::Title));
        assert!(matches!(fetched.sort_direction, SortDirection::Asc));
        assert_eq!(fetched.ollama_base_url, "http://custom:1234");
        assert_eq!(fetched.ollama_model.as_deref(), Some("llama3"));

        // Update theme.
        let updated = UserSettings {
            theme: Theme::Light,
            ..fetched
        };
        settings::upsert_user_settings(conn, &updated)?;

        let re_fetched = settings::get_user_settings(conn, uid)?;
        assert!(matches!(re_fetched.theme, Theme::Light));
        Ok(())
    })
    .unwrap();
}

#[test]
fn get_user_settings_nonexistent_returns_error() {
    let db = setup_db();
    db.with_connection(|conn| {
        let result = settings::get_user_settings(conn, 9999);
        assert!(result.is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn theme_enum_round_trips() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "theme_test");

        for theme in [Theme::Light, Theme::Dark, Theme::System] {
            let us = UserSettings {
                user_id: uid,
                theme: theme.clone(),
                last_collection_id: None,
                sidebar_collapsed: false,
                sort_field: SortField::UpdatedAt,
                sort_direction: SortDirection::Desc,
                ollama_base_url: "http://localhost:11434".to_owned(),
                ollama_model: None,
                extra: "{}".to_owned(),
            };
            settings::upsert_user_settings(conn, &us)?;
            let fetched = settings::get_user_settings(conn, uid)?;

            // Compare via debug string since Theme does not implement PartialEq.
            assert_eq!(format!("{:?}", fetched.theme), format!("{theme:?}"));
        }
        Ok(())
    })
    .unwrap();
}
