// =============================================================================
// Script version service integration tests.
//
// Verifies version restore workflow including pre-restore snapshot creation,
// content and script_language restoration, and version counter progression.
// =============================================================================

use neuronprompter_core::domain::script::{NewScript, UpdateScript};

use super::{create_test_user, setup_db};
use crate::{script_service, script_version_service};

/// Builds a minimal `NewScript` for testing.
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

/// Builds an `UpdateScript` with all fields set to None for the given script ID.
fn empty_update(script_id: i64) -> UpdateScript {
    UpdateScript {
        script_id,
        title: None,
        content: None,
        description: None,
        notes: None,
        script_language: None,
        language: None,
        source_path: None,
        is_synced: None,
        tag_ids: None,
        category_ids: None,
        collection_ids: None,
        expected_version: None,
    }
}

#[test]
fn list_script_versions() {
    let db = setup_db();
    let uid = create_test_user(&db, "listversionuser");

    let script =
        script_service::create_script(&db, &make_script(uid, "Version One", "python")).unwrap();

    // Edit 1: change title.
    let u1 = UpdateScript {
        title: Some("Version Two".to_owned()),
        ..empty_update(script.id)
    };
    script_service::update_script(&db, &u1).unwrap();

    // Edit 2: change content.
    let u2 = UpdateScript {
        content: Some("Updated content".to_owned()),
        ..empty_update(script.id)
    };
    script_service::update_script(&db, &u2).unwrap();

    let versions = script_version_service::list_versions(&db, script.id).unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].title, "Version One");
    assert_eq!(versions[0].version_number, 1);
    assert_eq!(versions[0].script_language, "python");
    assert_eq!(versions[1].title, "Version Two");
    assert_eq!(versions[1].version_number, 2);
}

#[test]
fn restore_script_version() {
    let db = setup_db();
    let uid = create_test_user(&db, "restoreuser");

    let script =
        script_service::create_script(&db, &make_script(uid, "Version One", "python")).unwrap();
    let original_content = script.content.clone();

    // Edit 1: change title.
    let u1 = UpdateScript {
        title: Some("Version Two".to_owned()),
        ..empty_update(script.id)
    };
    script_service::update_script(&db, &u1).unwrap();

    // Edit 2: change script_language.
    let u2 = UpdateScript {
        script_language: Some("bash".to_owned()),
        ..empty_update(script.id)
    };
    script_service::update_script(&db, &u2).unwrap();

    // Edit 3: change content.
    let u3 = UpdateScript {
        content: Some("Completely different content".to_owned()),
        ..empty_update(script.id)
    };
    script_service::update_script(&db, &u3).unwrap();

    // At this point we have 3 version rows and script is at version 4.
    let versions = script_version_service::list_versions(&db, script.id).unwrap();
    assert_eq!(versions.len(), 3);

    // Restore to version 1.
    let restored = script_version_service::restore_version(&db, script.id, 1).unwrap();

    // Content and script_language should match the creation state.
    assert_eq!(restored.title, "Version One");
    assert_eq!(restored.content, original_content);
    assert_eq!(restored.script_language, "python");

    // A pre-restore snapshot should have been created, totaling 4 version rows.
    let versions = script_version_service::list_versions(&db, script.id).unwrap();
    assert_eq!(versions.len(), 4);
}

#[test]
fn restore_creates_snapshot() {
    let db = setup_db();
    let uid = create_test_user(&db, "snapshotuser");

    let script =
        script_service::create_script(&db, &make_script(uid, "Original", "python")).unwrap();

    // Edit: change title and language.
    let u1 = UpdateScript {
        title: Some("Changed Title".to_owned()),
        script_language: Some("rust".to_owned()),
        ..empty_update(script.id)
    };
    script_service::update_script(&db, &u1).unwrap();

    // Restore to version 1.
    script_version_service::restore_version(&db, script.id, 1).unwrap();

    // The pre-restore snapshot should contain the changed state.
    let versions = script_version_service::list_versions(&db, script.id).unwrap();
    assert_eq!(versions.len(), 2);

    let pre_restore = &versions[1];
    assert_eq!(pre_restore.title, "Changed Title");
    assert_eq!(pre_restore.script_language, "rust");
}
