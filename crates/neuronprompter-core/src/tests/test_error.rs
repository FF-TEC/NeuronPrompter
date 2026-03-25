// =============================================================================
// Tests for CoreError Display formatting.
//
// Verifies that each error variant produces the expected human-readable message.
// =============================================================================

use crate::CoreError;

#[test]
fn validation_error_display() {
    let err = CoreError::Validation {
        field: "username".to_owned(),
        message: "must not be empty".to_owned(),
    };
    let display = format!("{err}");
    assert_eq!(
        display,
        "Validation error on field 'username': must not be empty"
    );
}

#[test]
fn not_found_error_display() {
    let err = CoreError::NotFound {
        entity: "Prompt".to_owned(),
        id: 42,
    };
    let display = format!("{err}");
    assert_eq!(display, "Prompt with id 42 not found");
}

#[test]
fn duplicate_error_display() {
    let err = CoreError::Duplicate {
        entity: "Tag".to_owned(),
        field: "name".to_owned(),
        value: "rust".to_owned(),
    };
    let display = format!("{err}");
    assert_eq!(display, "Duplicate Tag: name 'rust' already exists");
}

#[test]
fn entity_in_use_error_display() {
    let err = CoreError::EntityInUse {
        entity_type: "Prompt".to_owned(),
        entity_id: 42,
        referencing_titles: vec!["Chain A".to_owned(), "Chain B".to_owned()],
    };
    let display = format!("{err}");
    assert_eq!(display, "Prompt 42 is in use by: Chain A, Chain B");
}

#[test]
fn authorization_error_display() {
    let err = CoreError::Authorization {
        message: "user 1 cannot access prompt 99".to_owned(),
    };
    let display = format!("{err}");
    assert_eq!(display, "Authorization: user 1 cannot access prompt 99");
}

#[test]
fn error_clone_produces_identical_message() {
    let err = CoreError::Validation {
        field: "content".to_owned(),
        message: "must not be empty".to_owned(),
    };
    let cloned = err.clone();
    assert_eq!(format!("{err}"), format!("{cloned}"));
}

#[test]
fn error_debug_contains_variant_name() {
    let err = CoreError::NotFound {
        entity: "Prompt".to_owned(),
        id: 1,
    };
    let debug = format!("{err:?}");
    assert!(debug.contains("NotFound"));
    assert!(debug.contains("Prompt"));
}
