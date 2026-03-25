// =============================================================================
// Tests for domain type construction, serde serialization round-trips, and
// Clone correctness.
//
// Each domain struct is constructed, serialized to JSON, deserialized back,
// and verified to retain all field values. This ensures that serde attributes
// (rename_all, field names) are correctly configured.
// =============================================================================

use chrono::{DateTime, TimeZone, Utc};

use crate::domain::category::{Category, NewCategory};
use crate::domain::chain::{Chain, ChainFilter, ChainStep, ChainStepInput, NewChain, StepType};
use crate::domain::collection::{Collection, NewCollection};
use crate::domain::prompt::{
    NewPrompt, Prompt, PromptFilter, PromptWithAssociations, UpdatePrompt,
};
use crate::domain::script::{NewScript, Script, ScriptFilter};
use crate::domain::script_version::ScriptVersion;
use crate::domain::settings::{AppSetting, SortDirection, SortField, Theme, UserSettings};
use crate::domain::tag::{NewTag, Tag};
use crate::domain::user::{NewUser, User};
use crate::domain::version::PromptVersion;

fn utc(y: i32, m: u32, d: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()
}

#[allow(clippy::many_single_char_names)]
fn utc_hms(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, min, s).unwrap()
}

// ---------------------------------------------------------------------------
// User
// ---------------------------------------------------------------------------

#[test]
fn user_serde_round_trip() {
    let user = User {
        id: 1,
        username: "alice".to_owned(),
        display_name: "Alice".to_owned(),
        created_at: utc(2025, 1, 1),
        updated_at: utc(2025, 1, 1),
    };
    let json = serde_json::to_string(&user).expect("serialize");
    let deserialized: User = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.id, 1);
    assert_eq!(deserialized.username, "alice");
    assert_eq!(deserialized.display_name, "Alice");
}

#[test]
fn new_user_serde_round_trip() {
    let new = NewUser {
        username: "bob".to_owned(),
        display_name: "Bob".to_owned(),
    };
    let json = serde_json::to_string(&new).expect("serialize");
    let deserialized: NewUser = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.username, "bob");
}

#[test]
fn user_clone_independence() {
    let user = User {
        id: 1,
        username: "alice".to_owned(),
        display_name: "Alice".to_owned(),
        created_at: utc(2025, 1, 1),
        updated_at: utc(2025, 1, 1),
    };
    let mut cloned = user.clone();
    cloned.username = "modified".to_owned();
    assert_eq!(user.username, "alice");
    assert_eq!(cloned.username, "modified");
}

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

#[test]
fn prompt_serde_round_trip() {
    let prompt = Prompt {
        id: 10,
        user_id: 1,
        title: "Test Prompt".to_owned(),
        content: "Prompt content here".to_owned(),
        description: Some("A test prompt".to_owned()),
        notes: None,
        language: Some("en".to_owned()),
        is_favorite: true,
        is_archived: false,
        current_version: 3,
        created_at: utc(2025, 1, 1),
        updated_at: utc_hms(2025, 6, 15, 12, 0, 0),
    };
    let json = serde_json::to_string(&prompt).expect("serialize");
    let deserialized: Prompt = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.id, 10);
    assert_eq!(deserialized.title, "Test Prompt");
    assert!(deserialized.is_favorite);
    assert!(!deserialized.is_archived);
    assert_eq!(deserialized.current_version, 3);
    assert!(deserialized.notes.is_none());
}

#[test]
fn prompt_all_optional_fields_null() {
    let prompt = Prompt {
        id: 1,
        user_id: 1,
        title: "Minimal".to_owned(),
        content: "Content".to_owned(),
        description: None,
        notes: None,
        language: None,
        is_favorite: false,
        is_archived: false,
        current_version: 1,
        created_at: utc(2025, 1, 1),
        updated_at: utc(2025, 1, 1),
    };
    let json = serde_json::to_string(&prompt).expect("serialize");
    assert!(json.contains("null"));
    let deserialized: Prompt = serde_json::from_str(&json).expect("deserialize");
    assert!(deserialized.description.is_none());
    assert!(deserialized.notes.is_none());
    assert!(deserialized.language.is_none());
}

#[test]
fn new_prompt_serde_round_trip() {
    let new = NewPrompt {
        user_id: 1,
        title: "Title".to_owned(),
        content: "Content".to_owned(),
        description: None,
        notes: None,
        language: None,
        tag_ids: vec![1, 2, 3],
        category_ids: vec![],
        collection_ids: vec![10],
    };
    let json = serde_json::to_string(&new).expect("serialize");
    let deserialized: NewPrompt = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.tag_ids, vec![1, 2, 3]);
    assert!(deserialized.category_ids.is_empty());
    assert_eq!(deserialized.collection_ids, vec![10]);
}

#[test]
fn update_prompt_nested_options() {
    // UpdatePrompt uses Option<Option<String>> for nullable-clearable fields.
    // None = field not touched, Some(None) = set to NULL, Some(Some(v)) = set to v.
    let update = UpdatePrompt {
        prompt_id: 5,
        title: Some("Updated Title".to_owned()),
        content: None, // Leave content unchanged
        description: Some(Some("Updated desc".to_owned())),
        notes: None,
        language: Some(None), // Clear language
        tag_ids: Some(vec![1]),
        category_ids: None,
        collection_ids: None,
        expected_version: None,
    };
    let json = serde_json::to_string(&update).expect("serialize");
    let deserialized: UpdatePrompt = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.title, Some("Updated Title".to_owned()));
    assert!(deserialized.content.is_none());
    assert_eq!(
        deserialized.description,
        Some(Some("Updated desc".to_owned()))
    );
    // language was Some(None), serialized as null, deserialized back as Some(None)
    // (the custom deserializer correctly distinguishes null from absent)
    assert_eq!(deserialized.language, Some(None));
}

#[test]
fn update_prompt_null_vs_absent_from_json() {
    // Simulate client sending JSON with explicit null to clear a field.
    let json_with_null = r#"{"prompt_id":1,"description":null}"#;
    let update: UpdatePrompt = serde_json::from_str(json_with_null).expect("deserialize");
    // null -> Some(None) means "clear this field"
    assert_eq!(update.description, Some(None));
    // absent fields -> None means "don't touch"
    assert!(update.notes.is_none());
    assert!(update.language.is_none());
    assert!(update.title.is_none());

    // Simulate client sending JSON without the field at all.
    let json_without = r#"{"prompt_id":1}"#;
    let update2: UpdatePrompt = serde_json::from_str(json_without).expect("deserialize");
    assert!(update2.description.is_none());

    // Simulate client sending JSON with a string value.
    let json_with_value = r#"{"prompt_id":1,"description":"new desc"}"#;
    let update3: UpdatePrompt = serde_json::from_str(json_with_value).expect("deserialize");
    assert_eq!(update3.description, Some(Some("new desc".to_owned())));
}

#[test]
fn prompt_with_associations_contains_all_parts() {
    let prompt = Prompt {
        id: 1,
        user_id: 1,
        title: "T".to_owned(),
        content: "C".to_owned(),
        description: None,
        notes: None,
        language: None,
        is_favorite: false,
        is_archived: false,
        current_version: 1,
        created_at: utc(2025, 1, 1),
        updated_at: utc(2025, 1, 1),
    };
    let pwa = PromptWithAssociations {
        prompt,
        tags: vec![Tag {
            id: 1,
            user_id: 1,
            name: "rust".to_owned(),
            created_at: utc(2025, 1, 1),
        }],
        categories: vec![],
        collections: vec![Collection {
            id: 1,
            user_id: 1,
            name: "favorites".to_owned(),
            created_at: utc(2025, 1, 1),
        }],
    };
    let json = serde_json::to_string(&pwa).expect("serialize");
    let deserialized: PromptWithAssociations = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.tags.len(), 1);
    assert_eq!(deserialized.tags[0].name, "rust");
    assert!(deserialized.categories.is_empty());
    assert_eq!(deserialized.collections.len(), 1);
}

#[test]
fn prompt_filter_default_all_none() {
    let filter = PromptFilter::default();
    assert!(filter.user_id.is_none());
    assert!(filter.is_favorite.is_none());
    assert!(filter.is_archived.is_none());
    assert!(filter.collection_id.is_none());
    assert!(filter.category_id.is_none());
    assert!(filter.tag_id.is_none());
}

// ---------------------------------------------------------------------------
// Tag, Category, Collection
// ---------------------------------------------------------------------------

#[test]
fn tag_serde_round_trip() {
    let tag = Tag {
        id: 1,
        user_id: 1,
        name: "rust".to_owned(),
        created_at: utc(2025, 1, 1),
    };
    let json = serde_json::to_string(&tag).expect("serialize");
    let deserialized: Tag = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.name, "rust");
}

#[test]
fn new_tag_serde_round_trip() {
    let new = NewTag {
        user_id: 1,
        name: "python".to_owned(),
    };
    let json = serde_json::to_string(&new).expect("serialize");
    let deserialized: NewTag = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.name, "python");
}

#[test]
fn category_serde_round_trip() {
    let cat = Category {
        id: 1,
        user_id: 1,
        name: "Programming".to_owned(),
        created_at: utc(2025, 1, 1),
    };
    let json = serde_json::to_string(&cat).expect("serialize");
    let deserialized: Category = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.name, "Programming");
}

#[test]
fn new_category_serde_round_trip() {
    let new = NewCategory {
        user_id: 1,
        name: "AI".to_owned(),
    };
    let json = serde_json::to_string(&new).expect("serialize");
    let deserialized: NewCategory = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.name, "AI");
}

#[test]
fn collection_serde_round_trip() {
    let col = Collection {
        id: 1,
        user_id: 1,
        name: "Work".to_owned(),
        created_at: utc(2025, 1, 1),
    };
    let json = serde_json::to_string(&col).expect("serialize");
    let deserialized: Collection = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.name, "Work");
}

#[test]
fn new_collection_serde_round_trip() {
    let new = NewCollection {
        user_id: 1,
        name: "Personal".to_owned(),
    };
    let json = serde_json::to_string(&new).expect("serialize");
    let deserialized: NewCollection = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.name, "Personal");
}

// ---------------------------------------------------------------------------
// PromptVersion
// ---------------------------------------------------------------------------

#[test]
fn prompt_version_serde_round_trip() {
    let ver = PromptVersion {
        id: 1,
        prompt_id: 10,
        version_number: 2,
        title: "Old Title".to_owned(),
        content: "Old content".to_owned(),
        description: None,
        notes: Some("Author note".to_owned()),
        language: Some("en".to_owned()),
        created_at: utc(2025, 1, 1),
    };
    let json = serde_json::to_string(&ver).expect("serialize");
    let deserialized: PromptVersion = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.version_number, 2);
    assert_eq!(deserialized.title, "Old Title");
    assert_eq!(deserialized.notes, Some("Author note".to_owned()));
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[test]
fn app_setting_serde_round_trip() {
    let setting = AppSetting {
        key: "last_user_id".to_owned(),
        value: "1".to_owned(),
    };
    let json = serde_json::to_string(&setting).expect("serialize");
    let deserialized: AppSetting = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.key, "last_user_id");
    assert_eq!(deserialized.value, "1");
}

#[test]
fn user_settings_serde_round_trip() {
    let settings = UserSettings {
        user_id: 1,
        theme: Theme::Dark,
        last_collection_id: Some(5),
        sidebar_collapsed: true,
        sort_field: SortField::Title,
        sort_direction: SortDirection::Asc,
        ollama_base_url: "http://localhost:11434".to_owned(),
        ollama_model: Some("llama3".to_owned()),
        extra: "{}".to_owned(),
    };
    let json = serde_json::to_string(&settings).expect("serialize");
    let deserialized: UserSettings = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.theme, Theme::Dark);
    assert_eq!(deserialized.sort_field, SortField::Title);
    assert_eq!(deserialized.sort_direction, SortDirection::Asc);
    assert!(deserialized.sidebar_collapsed);
    assert_eq!(deserialized.last_collection_id, Some(5));
    assert_eq!(deserialized.ollama_model, Some("llama3".to_owned()));
}

#[test]
fn theme_serializes_lowercase() {
    assert_eq!(
        serde_json::to_string(&Theme::Light).expect("serialize"),
        "\"light\""
    );
    assert_eq!(
        serde_json::to_string(&Theme::Dark).expect("serialize"),
        "\"dark\""
    );
    assert_eq!(
        serde_json::to_string(&Theme::System).expect("serialize"),
        "\"system\""
    );
}

#[test]
fn sort_field_serializes_snake_case() {
    assert_eq!(
        serde_json::to_string(&SortField::UpdatedAt).expect("serialize"),
        "\"updated_at\""
    );
    assert_eq!(
        serde_json::to_string(&SortField::CreatedAt).expect("serialize"),
        "\"created_at\""
    );
    assert_eq!(
        serde_json::to_string(&SortField::Title).expect("serialize"),
        "\"title\""
    );
}

#[test]
fn sort_direction_serializes_lowercase() {
    assert_eq!(
        serde_json::to_string(&SortDirection::Asc).expect("serialize"),
        "\"asc\""
    );
    assert_eq!(
        serde_json::to_string(&SortDirection::Desc).expect("serialize"),
        "\"desc\""
    );
}

#[test]
fn user_settings_default_values() {
    // Verifies the expected default configuration for a fresh user.
    let settings = UserSettings {
        user_id: 1,
        theme: Theme::System,
        last_collection_id: None,
        sidebar_collapsed: false,
        sort_field: SortField::UpdatedAt,
        sort_direction: SortDirection::Desc,
        ollama_base_url: "http://localhost:11434".to_owned(),
        ollama_model: None,
        extra: "{}".to_owned(),
    };
    let json = serde_json::to_string(&settings).expect("serialize");
    let deserialized: UserSettings = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.theme, Theme::System);
    assert!(deserialized.last_collection_id.is_none());
    assert!(!deserialized.sidebar_collapsed);
    assert_eq!(deserialized.sort_field, SortField::UpdatedAt);
    assert_eq!(deserialized.sort_direction, SortDirection::Desc);
    assert!(deserialized.ollama_model.is_none());
}

// ---------------------------------------------------------------------------
// Chain domain types
// ---------------------------------------------------------------------------

#[test]
fn chain_serde_round_trip() {
    let chain = Chain {
        id: 1,
        user_id: 1,
        title: "My Chain".to_owned(),
        description: Some("A test chain".to_owned()),
        notes: None,
        language: Some("en".to_owned()),
        separator: "\n\n".to_owned(),
        is_favorite: true,
        is_archived: false,
        created_at: utc(2025, 1, 1),
        updated_at: utc(2025, 6, 1),
    };
    let json = serde_json::to_string(&chain).expect("serialize");
    let deserialized: Chain = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.title, "My Chain");
    assert_eq!(deserialized.separator, "\n\n");
    assert!(deserialized.is_favorite);
}

#[test]
fn chain_step_serde_round_trip() {
    let step = ChainStep {
        id: 1,
        chain_id: 1,
        step_type: StepType::Prompt,
        prompt_id: Some(42),
        script_id: None,
        position: 0,
    };
    let json = serde_json::to_string(&step).expect("serialize");
    assert!(json.contains("\"prompt\""));
    let deserialized: ChainStep = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.step_type, StepType::Prompt);
    assert_eq!(deserialized.prompt_id, Some(42));
    assert!(deserialized.script_id.is_none());
}

#[test]
fn chain_step_input_serde_round_trip() {
    let input = ChainStepInput {
        step_type: StepType::Script,
        item_id: 7,
    };
    let json = serde_json::to_string(&input).expect("serialize");
    assert!(json.contains("\"script\""));
    let deserialized: ChainStepInput = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.step_type, StepType::Script);
    assert_eq!(deserialized.item_id, 7);
}

#[test]
fn new_chain_serde_round_trip() {
    let new = NewChain {
        user_id: 1,
        title: "Chain".to_owned(),
        description: None,
        notes: None,
        language: None,
        separator: Some("---".to_owned()),
        prompt_ids: vec![1, 2],
        steps: vec![],
        tag_ids: vec![],
        category_ids: vec![],
        collection_ids: vec![],
    };
    let json = serde_json::to_string(&new).expect("serialize");
    let deserialized: NewChain = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.prompt_ids, vec![1, 2]);
}

#[test]
fn chain_filter_default() {
    let filter = ChainFilter::default();
    assert!(filter.user_id.is_none());
    assert!(filter.is_favorite.is_none());
    assert!(filter.tag_id.is_none());
}

// ---------------------------------------------------------------------------
// Script domain types
// ---------------------------------------------------------------------------

#[test]
fn script_serde_round_trip() {
    let script = Script {
        id: 1,
        user_id: 1,
        title: "Helper Script".to_owned(),
        content: "print('hello')".to_owned(),
        description: Some("A helper".to_owned()),
        notes: None,
        script_language: "python".to_owned(),
        language: Some("en".to_owned()),
        is_favorite: false,
        is_archived: false,
        current_version: 3,
        created_at: utc(2025, 1, 1),
        updated_at: utc(2025, 6, 1),
        source_path: Some("/home/user/script.py".to_owned()),
        is_synced: true,
    };
    let json = serde_json::to_string(&script).expect("serialize");
    let deserialized: Script = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.script_language, "python");
    assert_eq!(deserialized.current_version, 3);
    assert!(deserialized.is_synced);
    assert_eq!(
        deserialized.source_path,
        Some("/home/user/script.py".to_owned())
    );
}

#[test]
fn new_script_serde_round_trip() {
    let new = NewScript {
        user_id: 1,
        title: "New Script".to_owned(),
        content: "echo 'hi'".to_owned(),
        script_language: "bash".to_owned(),
        description: None,
        notes: None,
        language: None,
        source_path: None,
        is_synced: false,
        tag_ids: vec![1],
        category_ids: vec![],
        collection_ids: vec![2],
    };
    let json = serde_json::to_string(&new).expect("serialize");
    let deserialized: NewScript = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.script_language, "bash");
    assert_eq!(deserialized.tag_ids, vec![1]);
}

#[test]
fn script_filter_default() {
    let filter = ScriptFilter::default();
    assert!(filter.is_synced.is_none());
    assert!(filter.user_id.is_none());
}

// ---------------------------------------------------------------------------
// ScriptVersion domain type
// ---------------------------------------------------------------------------

#[test]
fn script_version_serde_round_trip() {
    let sv = ScriptVersion {
        id: 10,
        script_id: 5,
        version_number: 2,
        title: "V2 Title".to_owned(),
        content: "updated content".to_owned(),
        description: Some("Desc".to_owned()),
        notes: None,
        script_language: "javascript".to_owned(),
        language: Some("en".to_owned()),
        created_at: utc(2025, 3, 1),
    };
    let json = serde_json::to_string(&sv).expect("serialize");
    let deserialized: ScriptVersion = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.script_id, 5);
    assert_eq!(deserialized.version_number, 2);
    assert_eq!(deserialized.script_language, "javascript");
}

// ---------------------------------------------------------------------------
// CoreError PathTraversal variant
// ---------------------------------------------------------------------------

#[test]
fn path_traversal_error_display() {
    let err = crate::CoreError::PathTraversal {
        path: "/etc/passwd".to_owned(),
    };
    assert!(err.to_string().contains("/etc/passwd"));
}

#[test]
fn entity_in_use_error_display_prompt() {
    let err = crate::CoreError::EntityInUse {
        entity_type: "Prompt".to_owned(),
        entity_id: 42,
        referencing_titles: vec!["Chain A".to_owned(), "Chain B".to_owned()],
    };
    let msg = err.to_string();
    assert!(msg.contains("42"));
    assert!(msg.contains("Chain A"));
    assert!(msg.contains("Chain B"));
}

#[test]
fn entity_in_use_error_display_script() {
    let err = crate::CoreError::EntityInUse {
        entity_type: "Script".to_owned(),
        entity_id: 7,
        referencing_titles: vec!["My Chain".to_owned()],
    };
    let msg = err.to_string();
    assert!(msg.contains('7'));
    assert!(msg.contains("My Chain"));
}
