// =============================================================================
// I/O service integration tests.
//
// Verifies JSON and Markdown import/export round-trips, export envelope
// structure, auto-creation of missing entities on import, database backup,
// and error handling for invalid paths and malformed input.
// =============================================================================

use std::path::PathBuf;

use neuronprompter_core::domain::prompt::NewPrompt;

use super::{create_test_user, make_prompt, setup_db};
use crate::{
    category_service, collection_service, io_service, prompt_service, tag_service, version_service,
};

/// Creates a temporary directory that is automatically deleted when dropped.
fn temp_dir(name: &str) -> TempDir {
    TempDir::new(name)
}

/// Simple temporary directory wrapper for test isolation.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("neuronprompter_test_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&path).expect("temp dir creation");
        Self { path }
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

// ---------------------------------------------------------------------------
// JSON export/import
// ---------------------------------------------------------------------------

#[test]
fn json_export_import_round_trip() {
    let db = setup_db();
    let uid = create_test_user(&db, "exportuser");

    // Create tags, categories, collections.
    let tag = tag_service::create_tag(&db, uid, "round_trip_tag").unwrap();
    let cat = category_service::create_category(&db, uid, "round_trip_cat").unwrap();
    let col = collection_service::create_collection(&db, uid, "round_trip_col").unwrap();

    // Create a prompt with associations.
    let new = NewPrompt {
        user_id: uid,
        title: "Export Test".to_owned(),
        content: "Exportable content".to_owned(),
        description: Some("A test prompt for export".to_owned()),
        notes: Some("Some notes".to_owned()),
        language: Some("en".to_owned()),
        tag_ids: vec![tag.id],
        category_ids: vec![cat.id],
        collection_ids: vec![col.id],
    };
    let prompt = prompt_service::create_prompt(&db, &new).unwrap();

    // Set it as favorite to verify that flag survives round-trip.
    prompt_service::toggle_favorite(&db, prompt.id, true).unwrap();

    // Create a version entry by updating.
    let update = neuronprompter_core::domain::prompt::UpdatePrompt {
        prompt_id: prompt.id,
        title: Some("Export Test v2".to_owned()),
        content: None,
        description: None,
        notes: None,
        language: None,
        tag_ids: None,
        category_ids: None,
        collection_ids: None,
        expected_version: None,
    };
    prompt_service::update_prompt(&db, &update).unwrap();

    // Export to JSON.
    let tmp = temp_dir("json_rt");
    let export_path = tmp.path().join("export.json");
    io_service::export_json(&db, uid, &[prompt.id], &export_path).unwrap();

    // Verify the file exists and contains valid JSON.
    let json_str = std::fs::read_to_string(&export_path).unwrap();
    let envelope: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert!(envelope["exported_by"]["username"].as_str().is_some());
    assert!(envelope["exported_at"].as_str().is_some());

    // Import under a different user.
    let uid2 = create_test_user(&db, "importuser");
    let summary = io_service::import_json(&db, uid2, &export_path).unwrap();
    assert_eq!(summary.prompts_imported, 1);
    assert_eq!(
        summary.tags_created, 1,
        "tag should be auto-created for import user"
    );
    assert_eq!(summary.categories_created, 1);
    assert_eq!(summary.collections_created, 1);

    // Verify imported prompt data.
    let imported_prompts = prompt_service::list_prompts(
        &db,
        &neuronprompter_core::domain::prompt::PromptFilter {
            user_id: Some(uid2),
            is_favorite: None,
            is_archived: None,
            collection_id: None,
            category_id: None,
            tag_id: None,
            limit: None,
            offset: None,
            ..Default::default()
        },
    )
    .unwrap()
    .items;
    assert_eq!(imported_prompts.len(), 1);
    assert_eq!(imported_prompts[0].title, "Export Test v2");

    // Verify the imported prompt has version history.
    let imported_versions = version_service::list_versions(&db, imported_prompts[0].id).unwrap();
    assert_eq!(imported_versions.len(), 1);
    assert_eq!(imported_versions[0].title, "Export Test");
}

#[test]
fn json_export_envelope_has_provenance() {
    let db = setup_db();
    let uid = create_test_user(&db, "provenanceuser");

    let prompt = prompt_service::create_prompt(&db, &make_prompt(uid, "Provenance")).unwrap();

    let tmp = temp_dir("json_prov");
    let path = tmp.path().join("provenance.json");
    io_service::export_json(&db, uid, &[prompt.id], &path).unwrap();

    let json_str = std::fs::read_to_string(&path).unwrap();
    let envelope: io_service::ExportEnvelope = serde_json::from_str(&json_str).unwrap();

    assert_eq!(envelope.exported_by.username, "provenanceuser");
    assert!(!envelope.exported_at.is_empty());
    assert_eq!(envelope.prompts.len(), 1);
    assert_eq!(envelope.prompts[0].title, "Provenance");
}

#[test]
fn json_import_auto_creates_entities() {
    let db = setup_db();
    let uid1 = create_test_user(&db, "creator");
    let uid2 = create_test_user(&db, "importer");

    // Create a prompt with unique tags that the importer does not have.
    let tag = tag_service::create_tag(&db, uid1, "unique_tag").unwrap();
    let new = NewPrompt {
        user_id: uid1,
        title: "Transfer".to_owned(),
        content: "Transfer content".to_owned(),
        description: None,
        notes: None,
        language: None,
        tag_ids: vec![tag.id],
        category_ids: Vec::new(),
        collection_ids: Vec::new(),
    };
    let prompt = prompt_service::create_prompt(&db, &new).unwrap();

    let tmp = temp_dir("json_auto");
    let path = tmp.path().join("transfer.json");
    io_service::export_json(&db, uid1, &[prompt.id], &path).unwrap();

    let summary = io_service::import_json(&db, uid2, &path).unwrap();
    assert_eq!(summary.tags_created, 1);

    // Verify the tag was created under uid2.
    let tags = tag_service::list_tags(&db, uid2).unwrap();
    assert!(tags.iter().any(|t| t.name == "unique_tag"));
}

// ---------------------------------------------------------------------------
// Markdown export/import
// ---------------------------------------------------------------------------

#[test]
fn markdown_export_import_round_trip() {
    let db = setup_db();
    let uid = create_test_user(&db, "mduser");

    let tag = tag_service::create_tag(&db, uid, "md_tag").unwrap();
    let new = NewPrompt {
        user_id: uid,
        title: "Markdown Prompt".to_owned(),
        content: "# Heading\n\nBody text here.".to_owned(),
        description: Some("A markdown test".to_owned()),
        notes: None,
        language: Some("en".to_owned()),
        tag_ids: vec![tag.id],
        category_ids: Vec::new(),
        collection_ids: Vec::new(),
    };
    let prompt = prompt_service::create_prompt(&db, &new).unwrap();

    let tmp = temp_dir("md_rt");
    let export_dir = tmp.path().join("export");
    io_service::export_markdown(&db, uid, &[prompt.id], &export_dir).unwrap();

    // Verify manifest.json exists.
    assert!(export_dir.join("manifest.json").exists());

    // Verify at least one .md file exists.
    let md_file = export_dir.join("prompt_001.md");
    assert!(md_file.exists());

    // Verify the .md file contains YAML front-matter and content.
    let md_content = std::fs::read_to_string(&md_file).unwrap();
    assert!(md_content.starts_with("---\n"));
    assert!(md_content.contains("# Heading"));

    // Import into a different user.
    let uid2 = create_test_user(&db, "mdimporter");
    let summary = io_service::import_markdown(&db, uid2, &export_dir).unwrap();
    assert_eq!(summary.prompts_imported, 1);
    assert_eq!(summary.tags_created, 1);

    // Verify imported content.
    let imported = prompt_service::list_prompts(
        &db,
        &neuronprompter_core::domain::prompt::PromptFilter {
            user_id: Some(uid2),
            is_favorite: None,
            is_archived: None,
            collection_id: None,
            category_id: None,
            tag_id: None,
            limit: None,
            offset: None,
            ..Default::default()
        },
    )
    .unwrap()
    .items;
    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].title, "Markdown Prompt");
    assert!(imported[0].content.contains("# Heading"));
}

// ---------------------------------------------------------------------------
// Database backup
// ---------------------------------------------------------------------------

#[test]
fn backup_database_creates_copy() {
    let tmp = temp_dir("backup");
    let source = tmp.path().join("source.db");
    let target = tmp.path().join("backup.db");

    // Create a real SQLite database as source (backup API requires valid DB).
    let conn = neuronprompter_db::SqliteConnection::open(&source).unwrap();
    conn.execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY); INSERT INTO test VALUES (1);")
        .unwrap();
    drop(conn);

    io_service::backup_database(&source, &target).unwrap();
    assert!(target.exists());

    // Verify the backup is a valid SQLite database with the same data.
    let backup_conn = neuronprompter_db::SqliteConnection::open(&target).unwrap();
    let count: i64 = backup_conn
        .query_row("SELECT COUNT(*) FROM test", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

#[test]
fn import_nonexistent_file_returns_io_error() {
    let db = setup_db();
    let uid = create_test_user(&db, "erruser");

    let result = io_service::import_json(&db, uid, std::path::Path::new("/nonexistent/file.json"));
    assert!(result.is_err());
}

#[test]
fn import_malformed_json_returns_serialization_error() {
    let db = setup_db();
    let uid = create_test_user(&db, "malformed");

    let tmp = temp_dir("malformed");
    let path = tmp.path().join("bad.json");
    std::fs::write(&path, "{ not valid json !!!").unwrap();

    let result = io_service::import_json(&db, uid, &path);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Serialization") || err_msg.contains("serial"),
        "error should indicate serialization failure: {err_msg}"
    );
}

#[test]
fn backup_to_invalid_path_returns_io_error() {
    let result = io_service::backup_database(
        std::path::Path::new("/nonexistent/source.db"),
        std::path::Path::new("/nonexistent/target.db"),
    );
    assert!(result.is_err());
}
