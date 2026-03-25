// =============================================================================
// Version service integration tests.
//
// Verifies version restore workflow including pre-restore snapshot creation,
// content restoration, and version counter progression.
// =============================================================================

use neuronprompter_core::domain::prompt::UpdatePrompt;

use super::{create_test_user, make_prompt, setup_db};
use crate::{prompt_service, version_service};

/// Builds an `UpdatePrompt` with all fields set to None for the given prompt ID.
fn empty_update(prompt_id: i64) -> UpdatePrompt {
    UpdatePrompt {
        prompt_id,
        title: None,
        content: None,
        description: None,
        notes: None,
        language: None,
        tag_ids: None,
        category_ids: None,
        collection_ids: None,
        expected_version: None,
    }
}

#[test]
fn restore_version_reverts_content() {
    let db = setup_db();
    let uid = create_test_user(&db, "restoreuser");

    // Create prompt with known content.
    let prompt = prompt_service::create_prompt(&db, &make_prompt(uid, "Version One")).unwrap();
    let original_content = prompt.content.clone();

    // Edit 1: change title.
    let u1 = UpdatePrompt {
        title: Some("Version Two".to_owned()),
        ..empty_update(prompt.id)
    };
    prompt_service::update_prompt(&db, &u1).unwrap();

    // Edit 2: change title again.
    let u2 = UpdatePrompt {
        title: Some("Version Three".to_owned()),
        ..empty_update(prompt.id)
    };
    prompt_service::update_prompt(&db, &u2).unwrap();

    // Edit 3: change content.
    let u3 = UpdatePrompt {
        content: Some("Completely different content".to_owned()),
        ..empty_update(prompt.id)
    };
    prompt_service::update_prompt(&db, &u3).unwrap();

    // At this point we have 3 version rows (version 1, 2, 3) and prompt is at version 4.
    let versions = version_service::list_versions(&db, prompt.id).unwrap();
    assert_eq!(versions.len(), 3);

    // Restore to version 1.
    let restored = version_service::restore_version(&db, prompt.id, 1).unwrap();

    // Content should match the creation state.
    assert_eq!(restored.title, "Version One");
    assert_eq!(restored.content, original_content);

    // A pre-restore snapshot should have been created, totaling 4 version rows.
    let versions = version_service::list_versions(&db, prompt.id).unwrap();
    assert_eq!(versions.len(), 4);

    // The latest version row should contain the pre-restore state.
    let pre_restore = &versions[3];
    assert_eq!(pre_restore.title, "Version Three");
    assert_eq!(pre_restore.content, "Completely different content");
}

#[test]
fn get_version_by_id() {
    let db = setup_db();
    let uid = create_test_user(&db, "getveruser");

    let prompt = prompt_service::create_prompt(&db, &make_prompt(uid, "GetVer")).unwrap();

    // Create a version by updating.
    let u = UpdatePrompt {
        title: Some("Changed".to_owned()),
        ..empty_update(prompt.id)
    };
    prompt_service::update_prompt(&db, &u).unwrap();

    let versions = version_service::list_versions(&db, prompt.id).unwrap();
    assert_eq!(versions.len(), 1);

    let fetched = version_service::get_version(&db, versions[0].id).unwrap();
    assert_eq!(fetched.title, "GetVer");
    assert_eq!(fetched.version_number, 1);
}
