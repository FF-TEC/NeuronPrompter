// =============================================================================
// Prompt service integration tests.
//
// Verifies the prompt lifecycle: creation with validation, updates with
// versioning, no-change detection, association synchronization, duplication,
// favorite/archive toggling, and deletion.
// =============================================================================

use neuronprompter_core::domain::prompt::{NewPrompt, UpdatePrompt};

use super::{create_test_user, make_prompt, setup_db};
use crate::{category_service, collection_service, prompt_service, tag_service, version_service};

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
fn create_prompt_validates_fields() {
    let db = setup_db();
    let uid = create_test_user(&db, "validator");

    // Empty title should fail validation.
    let bad = NewPrompt {
        user_id: uid,
        title: String::new(),
        content: "Some content".to_owned(),
        ..make_prompt(uid, "unused")
    };
    let result = prompt_service::create_prompt(&db, &bad);
    assert!(result.is_err());
}

#[test]
fn create_prompt_with_associations() {
    let db = setup_db();
    let uid = create_test_user(&db, "assocuser");

    let tag = tag_service::create_tag(&db, uid, "rust").unwrap();
    let cat = category_service::create_category(&db, uid, "programming").unwrap();
    let col = collection_service::create_collection(&db, uid, "favorites").unwrap();

    let new = NewPrompt {
        user_id: uid,
        title: "Tagged Prompt".to_owned(),
        content: "Prompt with tags".to_owned(),
        description: None,
        notes: None,
        language: Some("en".to_owned()),
        tag_ids: vec![tag.id],
        category_ids: vec![cat.id],
        collection_ids: vec![col.id],
    };
    let prompt = prompt_service::create_prompt(&db, &new).unwrap();

    let pwa = prompt_service::get_prompt(&db, prompt.id).unwrap();
    assert_eq!(pwa.tags.len(), 1);
    assert_eq!(pwa.tags[0].name, "rust");
    assert_eq!(pwa.categories.len(), 1);
    assert_eq!(pwa.categories[0].name, "programming");
    assert_eq!(pwa.collections.len(), 1);
    assert_eq!(pwa.collections[0].name, "favorites");
}

#[test]
fn update_prompt_creates_version_snapshot() {
    let db = setup_db();
    let uid = create_test_user(&db, "versionuser");

    // Create prompt (version = 1, no version rows).
    let prompt = prompt_service::create_prompt(&db, &make_prompt(uid, "Original")).unwrap();
    assert_eq!(prompt.current_version, 1);

    // Update title -> should create 1 version row with original data.
    let update = UpdatePrompt {
        title: Some("Updated Title".to_owned()),
        ..empty_update(prompt.id)
    };
    let updated = prompt_service::update_prompt(&db, &update).unwrap();
    assert_eq!(updated.title, "Updated Title");

    let versions = version_service::list_versions(&db, prompt.id).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].title, "Original");
    assert_eq!(versions[0].version_number, 1);

    // Update content -> should create a second version row.
    let second_update = UpdatePrompt {
        content: Some("Brand new content".to_owned()),
        ..empty_update(prompt.id)
    };
    prompt_service::update_prompt(&db, &second_update).unwrap();

    let versions = version_service::list_versions(&db, prompt.id).unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[1].title, "Updated Title");
}

#[test]
fn no_change_update_skips_versioning() {
    let db = setup_db();
    let uid = create_test_user(&db, "nochangeuser");

    let prompt = prompt_service::create_prompt(&db, &make_prompt(uid, "Static")).unwrap();

    // Send an update with identical title and content.
    let update = UpdatePrompt {
        title: Some("Static".to_owned()),
        content: Some("Content for Static".to_owned()),
        ..empty_update(prompt.id)
    };
    let result = prompt_service::update_prompt(&db, &update).unwrap();

    // No version row should have been created.
    let versions = version_service::list_versions(&db, prompt.id).unwrap();
    assert!(
        versions.is_empty(),
        "no version should be created for identical update"
    );

    // Prompt content should remain unchanged.
    assert_eq!(result.title, "Static");
}

#[test]
fn association_sync_adds_and_removes() {
    let db = setup_db();
    let uid = create_test_user(&db, "syncuser");

    let tag_a = tag_service::create_tag(&db, uid, "tag_a").unwrap();
    let tag_b = tag_service::create_tag(&db, uid, "tag_b").unwrap();
    let tag_c = tag_service::create_tag(&db, uid, "tag_c").unwrap();

    // Create prompt with tags [A, B].
    let new = NewPrompt {
        user_id: uid,
        title: "Sync Test".to_owned(),
        content: "Content".to_owned(),
        description: None,
        notes: None,
        language: None,
        tag_ids: vec![tag_a.id, tag_b.id],
        category_ids: Vec::new(),
        collection_ids: Vec::new(),
    };
    let prompt = prompt_service::create_prompt(&db, &new).unwrap();

    // Verify initial tags [A, B].
    let pwa = prompt_service::get_prompt(&db, prompt.id).unwrap();
    let tag_names: Vec<&str> = pwa.tags.iter().map(|t| t.name.as_str()).collect();
    assert!(tag_names.contains(&"tag_a"));
    assert!(tag_names.contains(&"tag_b"));

    // Update to tags [B, C] (remove A, add C).
    let update = UpdatePrompt {
        tag_ids: Some(vec![tag_b.id, tag_c.id]),
        ..empty_update(prompt.id)
    };
    prompt_service::update_prompt(&db, &update).unwrap();

    // Verify tags are [B, C].
    let pwa = prompt_service::get_prompt(&db, prompt.id).unwrap();
    let tag_names: Vec<&str> = pwa.tags.iter().map(|t| t.name.as_str()).collect();
    assert!(
        !tag_names.contains(&"tag_a"),
        "tag_a should have been removed"
    );
    assert!(tag_names.contains(&"tag_b"), "tag_b should remain");
    assert!(tag_names.contains(&"tag_c"), "tag_c should have been added");
    assert_eq!(pwa.tags.len(), 2);
}

#[test]
fn duplicate_prompt_copies_data() {
    let db = setup_db();
    let uid = create_test_user(&db, "dupuser");

    let tag = tag_service::create_tag(&db, uid, "shared_tag").unwrap();
    let new = NewPrompt {
        user_id: uid,
        title: "Original".to_owned(),
        content: "Original content".to_owned(),
        description: Some("A description".to_owned()),
        notes: None,
        language: Some("en".to_owned()),
        tag_ids: vec![tag.id],
        category_ids: Vec::new(),
        collection_ids: Vec::new(),
    };
    let prompt = prompt_service::create_prompt(&db, &new).unwrap();

    let copy = prompt_service::duplicate_prompt(&db, prompt.id).unwrap();
    assert!(
        copy.title.contains("(copy)"),
        "duplicate title should contain '(copy)'"
    );
    assert_eq!(copy.content, "Original content");
    assert_eq!(
        copy.current_version, 1,
        "duplicate should reset version to 1"
    );
    assert_ne!(copy.id, prompt.id, "duplicate should have a different ID");
}

#[test]
fn toggle_favorite_and_archive() {
    let db = setup_db();
    let uid = create_test_user(&db, "toggleuser");

    let prompt = prompt_service::create_prompt(&db, &make_prompt(uid, "Toggle Test")).unwrap();
    assert!(!prompt.is_favorite);
    assert!(!prompt.is_archived);

    prompt_service::toggle_favorite(&db, prompt.id, true).unwrap();
    let pwa = prompt_service::get_prompt(&db, prompt.id).unwrap();
    assert!(pwa.prompt.is_favorite);

    prompt_service::toggle_archive(&db, prompt.id, true).unwrap();
    let pwa = prompt_service::get_prompt(&db, prompt.id).unwrap();
    assert!(pwa.prompt.is_archived);

    // Toggle back.
    prompt_service::toggle_favorite(&db, prompt.id, false).unwrap();
    prompt_service::toggle_archive(&db, prompt.id, false).unwrap();
    let pwa = prompt_service::get_prompt(&db, prompt.id).unwrap();
    assert!(!pwa.prompt.is_favorite);
    assert!(!pwa.prompt.is_archived);
}

#[test]
fn delete_prompt_removes_all_data() {
    let db = setup_db();
    let uid = create_test_user(&db, "deluser");

    let prompt = prompt_service::create_prompt(&db, &make_prompt(uid, "Deletable")).unwrap();
    prompt_service::delete_prompt(&db, prompt.id).unwrap();

    let result = prompt_service::get_prompt(&db, prompt.id);
    assert!(result.is_err(), "deleted prompt should not be found");
}
