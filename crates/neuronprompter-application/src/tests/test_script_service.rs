// =============================================================================
// Script service integration tests.
//
// Verifies the script lifecycle: creation with validation, updates with
// versioning, no-change detection, deletion guarded by chain references,
// duplication, and favorite/archive toggling.
// =============================================================================

use neuronprompter_core::domain::chain::{ChainStepInput, NewChain};
use neuronprompter_core::domain::script::{NewScript, UpdateScript};

use super::{create_test_user, setup_db};
use crate::{chain_service, script_service, script_version_service};

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
fn create_script_validates_title() {
    let db = setup_db();
    let uid = create_test_user(&db, "titleval");

    let bad = NewScript {
        title: String::new(),
        ..make_script(uid, "unused", "python")
    };
    let result = script_service::create_script(&db, &bad);
    assert!(result.is_err(), "empty title should fail validation");
}

#[test]
fn create_script_validates_script_language() {
    let db = setup_db();
    let uid = create_test_user(&db, "langval");

    let bad = NewScript {
        script_language: String::new(),
        ..make_script(uid, "Lang Test", "unused")
    };
    let result = script_service::create_script(&db, &bad);
    assert!(
        result.is_err(),
        "empty script_language should fail validation"
    );
}

#[test]
fn create_script_validates_content() {
    let db = setup_db();
    let uid = create_test_user(&db, "contentval");

    let bad = NewScript {
        content: String::new(),
        ..make_script(uid, "Content Test", "python")
    };
    let result = script_service::create_script(&db, &bad);
    assert!(result.is_err(), "empty content should fail validation");
}

#[test]
fn create_and_get_script() {
    let db = setup_db();
    let uid = create_test_user(&db, "cruduser");

    let script =
        script_service::create_script(&db, &make_script(uid, "My Script", "python")).unwrap();
    assert_eq!(script.title, "My Script");
    assert_eq!(script.script_language, "python");
    assert_eq!(script.current_version, 1);

    let swa = script_service::get_script(&db, script.id).unwrap();
    assert_eq!(swa.script.title, "My Script");
    assert_eq!(swa.script.script_language, "python");
}

#[test]
fn update_script_creates_version() {
    let db = setup_db();
    let uid = create_test_user(&db, "versionuser");

    let script =
        script_service::create_script(&db, &make_script(uid, "Original", "python")).unwrap();
    assert_eq!(script.current_version, 1);

    // Update title -> should create 1 version row with original data.
    let update = UpdateScript {
        title: Some("Updated Title".to_owned()),
        ..empty_update(script.id)
    };
    let updated = script_service::update_script(&db, &update).unwrap();
    assert_eq!(updated.title, "Updated Title");

    let versions = script_version_service::list_versions(&db, script.id).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].title, "Original");
    assert_eq!(versions[0].version_number, 1);
    assert_eq!(versions[0].script_language, "python");

    // Update content -> should create a second version row.
    let second_update = UpdateScript {
        content: Some("Brand new content".to_owned()),
        ..empty_update(script.id)
    };
    script_service::update_script(&db, &second_update).unwrap();

    let versions = script_version_service::list_versions(&db, script.id).unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[1].title, "Updated Title");
}

#[test]
fn update_script_no_change_skips_version() {
    let db = setup_db();
    let uid = create_test_user(&db, "nochangeuser");

    let script = script_service::create_script(&db, &make_script(uid, "Static", "python")).unwrap();

    // Send an update with identical title, content, and language.
    let update = UpdateScript {
        title: Some("Static".to_owned()),
        content: Some("# Content for Static".to_owned()),
        script_language: Some("python".to_owned()),
        ..empty_update(script.id)
    };
    let result = script_service::update_script(&db, &update).unwrap();

    // No version row should have been created.
    let versions = script_version_service::list_versions(&db, script.id).unwrap();
    assert!(
        versions.is_empty(),
        "no version should be created for identical update"
    );

    assert_eq!(result.title, "Static");
}

#[test]
fn delete_script_in_use_fails() {
    let db = setup_db();
    let uid = create_test_user(&db, "inuseuser");

    let script =
        script_service::create_script(&db, &make_script(uid, "In Use Script", "python")).unwrap();

    // Create a chain referencing this script.
    let chain_new = NewChain {
        user_id: uid,
        title: "Blocker Chain".to_owned(),
        description: None,
        notes: None,
        language: None,
        separator: None,
        prompt_ids: Vec::new(),
        steps: vec![ChainStepInput {
            step_type: neuronprompter_core::domain::chain::StepType::Script,
            item_id: script.id,
        }],
        tag_ids: Vec::new(),
        category_ids: Vec::new(),
        collection_ids: Vec::new(),
    };
    chain_service::create_chain(&db, &chain_new).unwrap();

    let result = script_service::delete_script(&db, script.id);
    assert!(
        result.is_err(),
        "deleting a script referenced by a chain should fail"
    );
}

#[test]
fn delete_script_succeeds() {
    let db = setup_db();
    let uid = create_test_user(&db, "deluser");

    let script =
        script_service::create_script(&db, &make_script(uid, "Deletable", "bash")).unwrap();
    script_service::delete_script(&db, script.id).unwrap();

    let result = script_service::get_script(&db, script.id);
    assert!(result.is_err(), "deleted script should not be found");
}

#[test]
fn duplicate_script() {
    let db = setup_db();
    let uid = create_test_user(&db, "dupuser");

    let script = script_service::create_script(
        &db,
        &NewScript {
            description: Some("A description".to_owned()),
            ..make_script(uid, "Original", "python")
        },
    )
    .unwrap();

    let copy = script_service::duplicate_script(&db, script.id).unwrap();
    assert!(
        copy.title.contains("(copy)"),
        "duplicate title should contain '(copy)'"
    );
    assert_eq!(copy.content, script.content);
    assert_eq!(copy.script_language, "python");
    assert_eq!(
        copy.current_version, 1,
        "duplicate should reset version to 1"
    );
    assert_ne!(copy.id, script.id, "duplicate should have a different ID");
}

#[test]
fn toggle_favorite_and_archive() {
    let db = setup_db();
    let uid = create_test_user(&db, "toggleuser");

    let script =
        script_service::create_script(&db, &make_script(uid, "Toggle Test", "python")).unwrap();
    assert!(!script.is_favorite);
    assert!(!script.is_archived);

    script_service::toggle_favorite(&db, script.id, true).unwrap();
    let swa = script_service::get_script(&db, script.id).unwrap();
    assert!(swa.script.is_favorite);

    script_service::toggle_archive(&db, script.id, true).unwrap();
    let swa = script_service::get_script(&db, script.id).unwrap();
    assert!(swa.script.is_archived);

    // Toggle back.
    script_service::toggle_favorite(&db, script.id, false).unwrap();
    script_service::toggle_archive(&db, script.id, false).unwrap();
    let swa = script_service::get_script(&db, script.id).unwrap();
    assert!(!swa.script.is_favorite);
    assert!(!swa.script.is_archived);
}
