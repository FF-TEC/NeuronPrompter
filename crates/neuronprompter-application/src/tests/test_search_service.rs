// =============================================================================
// Tests for search_service: FTS5 search through the service layer.
// =============================================================================

use neuronprompter_core::domain::prompt::PromptFilter;
use neuronprompter_core::domain::script::ScriptFilter;

use super::{create_test_user, make_prompt, make_script, setup_db};
use crate::{prompt_service, script_service, search_service};

#[test]
fn search_prompts_finds_matching_content() {
    let db = setup_db();
    let uid = create_test_user(&db, "searcher");

    let mut p = make_prompt(uid, "Quantum Computing Guide");
    p.content = "Explain quantum entanglement in simple terms".to_owned();
    prompt_service::create_prompt(&db, &p).expect("create");

    let filter = PromptFilter {
        user_id: Some(uid),
        ..Default::default()
    };
    let results = search_service::search_prompts(&db, uid, "quantum", &filter).expect("search");
    assert!(!results.is_empty(), "should find the prompt");
    assert_eq!(results[0].title, "Quantum Computing Guide");
}

#[test]
fn search_prompts_returns_empty_for_no_match() {
    let db = setup_db();
    let uid = create_test_user(&db, "searcher2");

    prompt_service::create_prompt(&db, &make_prompt(uid, "Hello World")).expect("create");

    let filter = PromptFilter {
        user_id: Some(uid),
        ..Default::default()
    };
    let results =
        search_service::search_prompts(&db, uid, "nonexistentxyz", &filter).expect("search");
    assert!(results.is_empty());
}

#[test]
fn search_prompts_empty_query_returns_empty() {
    let db = setup_db();
    let uid = create_test_user(&db, "searcher3");

    prompt_service::create_prompt(&db, &make_prompt(uid, "Prompt A")).expect("create");

    let filter = PromptFilter::default();
    let results = search_service::search_prompts(&db, uid, "", &filter).expect("search");
    assert!(
        results.is_empty(),
        "empty query should return empty results"
    );
}

#[test]
fn search_scripts_finds_matching_content() {
    let db = setup_db();
    let uid = create_test_user(&db, "scriptsearcher");

    let mut s = make_script(uid, "Deploy Script", "bash");
    s.content = "#!/bin/bash\nkubectl apply -f deployment.yaml".to_owned();
    script_service::create_script(&db, &s).expect("create");

    let filter = ScriptFilter {
        user_id: Some(uid),
        ..Default::default()
    };
    let results = search_service::search_scripts(&db, uid, "kubectl", &filter).expect("search");
    assert!(!results.is_empty(), "should find the script");
    assert_eq!(results[0].title, "Deploy Script");
}
