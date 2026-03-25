// =============================================================================
// Chain repository integration tests.
//
// Verifies CRUD operations, step management, composed content, taxonomy
// associations, chain-prompt reference lookups, duplication, favorite/archive
// toggling, FTS5 search, and cascade deletion.
// =============================================================================

use neuronprompter_core::domain::chain::{ChainFilter, NewChain};
use neuronprompter_core::domain::prompt::NewPrompt;
use neuronprompter_core::domain::user::NewUser;

use super::setup_db;
use crate::ConnectionProvider;
use crate::repo::{chains, prompts, tags, users};

fn create_user(conn: &rusqlite::Connection, username: &str) -> i64 {
    let new = NewUser {
        username: username.to_owned(),
        display_name: format!("Display {username}"),
    };
    users::create_user(conn, &new).expect("user creation").id
}

fn make_prompt(user_id: i64, title: &str, content: &str) -> NewPrompt {
    NewPrompt {
        user_id,
        title: title.to_owned(),
        content: content.to_owned(),
        description: None,
        notes: None,
        language: None,
        tag_ids: Vec::new(),
        category_ids: Vec::new(),
        collection_ids: Vec::new(),
    }
}

fn make_chain(user_id: i64, title: &str, prompt_ids: Vec<i64>) -> NewChain {
    NewChain {
        user_id,
        title: title.to_owned(),
        description: None,
        notes: None,
        language: None,
        separator: None,
        prompt_ids,
        steps: Vec::new(),
        tag_ids: Vec::new(),
        category_ids: Vec::new(),
        collection_ids: Vec::new(),
    }
}

// ---- Create & Get ----

#[test]
fn create_and_get_chain() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "chainuser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "Hello"))?;
        let p2 = prompts::create_prompt(conn, &make_prompt(uid, "P2", "World"))?;

        let chain = chains::create_chain(conn, &make_chain(uid, "My Chain", vec![p1.id, p2.id]))?;

        assert_eq!(chain.title, "My Chain");
        assert_eq!(chain.user_id, uid);
        assert_eq!(chain.separator, "\n\n");
        assert!(!chain.is_favorite);
        assert!(!chain.is_archived);

        let fetched = chains::get_chain(conn, chain.id)?;
        assert_eq!(fetched.title, "My Chain");
        Ok(())
    })
    .unwrap();
}

#[test]
fn get_chain_with_steps() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "stepsuser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "Prompt A", "Content A"))?;
        let p2 = prompts::create_prompt(conn, &make_prompt(uid, "Prompt B", "Content B"))?;
        let p3 = prompts::create_prompt(conn, &make_prompt(uid, "Prompt C", "Content C"))?;

        let chain = chains::create_chain(
            conn,
            &make_chain(uid, "Three Steps", vec![p1.id, p2.id, p3.id]),
        )?;

        let detail = chains::get_chain_with_steps(conn, chain.id)?;
        assert_eq!(detail.steps.len(), 3);
        assert_eq!(detail.steps[0].step.position, 0);
        assert_eq!(detail.steps[0].prompt.as_ref().unwrap().title, "Prompt A");
        assert_eq!(detail.steps[1].step.position, 1);
        assert_eq!(detail.steps[1].prompt.as_ref().unwrap().title, "Prompt B");
        assert_eq!(detail.steps[2].step.position, 2);
        assert_eq!(detail.steps[2].prompt.as_ref().unwrap().title, "Prompt C");
        Ok(())
    })
    .unwrap();
}

// ---- Composed Content ----

#[test]
fn composed_content_default_separator() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "composeduser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "First part"))?;
        let p2 = prompts::create_prompt(conn, &make_prompt(uid, "P2", "Second part"))?;

        let chain = chains::create_chain(conn, &make_chain(uid, "Composed", vec![p1.id, p2.id]))?;

        let content = chains::get_composed_content(conn, chain.id)?;
        assert_eq!(content, "First part\n\nSecond part");
        Ok(())
    })
    .unwrap();
}

#[test]
fn composed_content_custom_separator() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "customsep");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "Alpha"))?;
        let p2 = prompts::create_prompt(conn, &make_prompt(uid, "P2", "Beta"))?;

        let new = NewChain {
            user_id: uid,
            title: "Custom Sep".to_owned(),
            separator: Some("\n---\n".to_owned()),
            prompt_ids: vec![p1.id, p2.id],
            ..make_chain(uid, "", vec![])
        };
        let chain = chains::create_chain(conn, &new)?;

        let content = chains::get_composed_content(conn, chain.id)?;
        assert_eq!(content, "Alpha\n---\nBeta");
        Ok(())
    })
    .unwrap();
}

// ---- Live Reference: prompt edits reflect in chain ----

#[test]
fn chain_reflects_prompt_edits() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "liveref");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "Original"))?;

        let chain = chains::create_chain(conn, &make_chain(uid, "Live", vec![p1.id]))?;

        let before = chains::get_composed_content(conn, chain.id)?;
        assert_eq!(before, "Original");

        // Update the prompt content.
        prompts::update_prompt_fields(conn, p1.id, None, Some("Updated"), None, None, None, None)?;

        let after = chains::get_composed_content(conn, chain.id)?;
        assert_eq!(after, "Updated");
        Ok(())
    })
    .unwrap();
}

// ---- Duplicate Prompts in Chain ----

#[test]
fn chain_allows_duplicate_prompts() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "dupuser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "Repeated"))?;

        let chain =
            chains::create_chain(conn, &make_chain(uid, "Dups", vec![p1.id, p1.id, p1.id]))?;

        let detail = chains::get_chain_with_steps(conn, chain.id)?;
        assert_eq!(detail.steps.len(), 3);
        assert_eq!(detail.steps[0].step.prompt_id, Some(p1.id));
        assert_eq!(detail.steps[1].step.prompt_id, Some(p1.id));
        assert_eq!(detail.steps[2].step.prompt_id, Some(p1.id));

        let content = chains::get_composed_content(conn, chain.id)?;
        assert_eq!(content, "Repeated\n\nRepeated\n\nRepeated");
        Ok(())
    })
    .unwrap();
}

// ---- Replace Steps ----

#[test]
fn replace_chain_steps() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "replaceuser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "A"))?;
        let p2 = prompts::create_prompt(conn, &make_prompt(uid, "P2", "B"))?;
        let p3 = prompts::create_prompt(conn, &make_prompt(uid, "P3", "C"))?;

        let chain = chains::create_chain(conn, &make_chain(uid, "Replace", vec![p1.id, p2.id]))?;

        // Replace with different order and different prompts.
        chains::replace_chain_steps(conn, chain.id, &[p3.id, p1.id])?;

        let detail = chains::get_chain_with_steps(conn, chain.id)?;
        assert_eq!(detail.steps.len(), 2);
        assert_eq!(detail.steps[0].prompt.as_ref().unwrap().title, "P3");
        assert_eq!(detail.steps[1].prompt.as_ref().unwrap().title, "P1");

        let content = chains::get_composed_content(conn, chain.id)?;
        assert_eq!(content, "C\n\nA");
        Ok(())
    })
    .unwrap();
}

// ---- Update Chain Metadata ----

#[test]
fn update_chain_fields() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "updateuser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "X"))?;

        let chain = chains::create_chain(conn, &make_chain(uid, "Original Title", vec![p1.id]))?;

        let updated = chains::update_chain_fields(
            conn,
            chain.id,
            Some("New Title"),
            Some(Some("Description")),
            None,
            None,
            Some("---"),
        )?;

        assert_eq!(updated.title, "New Title");
        assert_eq!(updated.description.as_deref(), Some("Description"));
        assert_eq!(updated.separator, "---");
        Ok(())
    })
    .unwrap();
}

// ---- Favorite / Archive ----

#[test]
fn toggle_chain_favorite() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "favuser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "X"))?;
        let chain = chains::create_chain(conn, &make_chain(uid, "Fav Chain", vec![p1.id]))?;

        assert!(!chain.is_favorite);

        chains::set_chain_favorite(conn, chain.id, true)?;
        let fetched = chains::get_chain(conn, chain.id)?;
        assert!(fetched.is_favorite);

        chains::set_chain_favorite(conn, chain.id, false)?;
        let fetched = chains::get_chain(conn, chain.id)?;
        assert!(!fetched.is_favorite);
        Ok(())
    })
    .unwrap();
}

#[test]
fn toggle_chain_archived() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "archuser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "X"))?;
        let chain = chains::create_chain(conn, &make_chain(uid, "Arch Chain", vec![p1.id]))?;

        chains::set_chain_archived(conn, chain.id, true)?;
        let fetched = chains::get_chain(conn, chain.id)?;
        assert!(fetched.is_archived);
        Ok(())
    })
    .unwrap();
}

// ---- Duplicate ----

#[test]
fn duplicate_chain() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "dupchainuser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "X"))?;
        let p2 = prompts::create_prompt(conn, &make_prompt(uid, "P2", "Y"))?;

        let tag = tags::create_tag(conn, uid, "test-tag")?;
        let mut new = make_chain(uid, "Original", vec![p1.id, p2.id]);
        new.tag_ids = vec![tag.id];
        let chain = chains::create_chain(conn, &new)?;

        let dup = chains::duplicate_chain(conn, chain.id)?;
        assert_eq!(dup.title, "Original (copy)");
        assert_ne!(dup.id, chain.id);

        // Verify steps were copied.
        let detail = chains::get_chain_with_steps(conn, dup.id)?;
        assert_eq!(detail.steps.len(), 2);
        assert_eq!(detail.steps[0].prompt.as_ref().unwrap().title, "P1");
        assert_eq!(detail.steps[1].prompt.as_ref().unwrap().title, "P2");

        // Verify tags were copied.
        assert_eq!(detail.tags.len(), 1);
        assert_eq!(detail.tags[0].name, "test-tag");
        Ok(())
    })
    .unwrap();
}

// ---- Delete ----

#[test]
fn delete_chain_cascades_steps() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "delchainuser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "X"))?;
        let chain = chains::create_chain(conn, &make_chain(uid, "To Delete", vec![p1.id]))?;

        chains::delete_chain(conn, chain.id)?;

        // Chain should be gone.
        let result = chains::get_chain(conn, chain.id);
        assert!(result.is_err());

        // Steps should be gone (no orphans).
        let count = chains::count_steps(conn, chain.id)?;
        assert_eq!(count, 0);

        // The prompt itself should NOT be deleted.
        let prompt = prompts::get_prompt(conn, p1.id)?;
        assert_eq!(prompt.title, "P1");
        Ok(())
    })
    .unwrap();
}

// ---- Prompt deletion blocked by RESTRICT ----

#[test]
fn delete_prompt_blocked_by_chain_reference() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "restrictuser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "X"))?;
        let _chain = chains::create_chain(conn, &make_chain(uid, "Blocker", vec![p1.id]))?;

        // Attempting to delete the prompt should fail due to RESTRICT.
        let result = prompts::delete_prompt(conn, p1.id);
        assert!(
            result.is_err(),
            "Prompt deletion should be blocked by RESTRICT"
        );
        Ok(())
    })
    .unwrap();
}

// ---- Chain-Prompt Reference Lookups ----

#[test]
fn get_chains_containing_prompt() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "refuser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "X"))?;
        let p2 = prompts::create_prompt(conn, &make_prompt(uid, "P2", "Y"))?;

        let _c1 = chains::create_chain(conn, &make_chain(uid, "Chain A", vec![p1.id, p2.id]))?;
        let _c2 = chains::create_chain(conn, &make_chain(uid, "Chain B", vec![p1.id]))?;
        let _c3 = chains::create_chain(conn, &make_chain(uid, "Chain C", vec![p2.id]))?;

        let containing_p1 = chains::get_chains_containing_prompt(conn, p1.id)?;
        assert_eq!(containing_p1.len(), 2);
        let titles: Vec<&str> = containing_p1.iter().map(|c| c.title.as_str()).collect();
        assert!(titles.contains(&"Chain A"));
        assert!(titles.contains(&"Chain B"));

        let containing_p2 = chains::get_chains_containing_prompt(conn, p2.id)?;
        assert_eq!(containing_p2.len(), 2);

        let count = chains::count_chains_for_prompt(conn, p1.id)?;
        assert_eq!(count, 2);
        Ok(())
    })
    .unwrap();
}

// ---- List & Filter ----

#[test]
fn list_chains_with_filter() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "filteruser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "X"))?;

        let c1 = chains::create_chain(conn, &make_chain(uid, "Chain 1", vec![p1.id]))?;
        let _c2 = chains::create_chain(conn, &make_chain(uid, "Chain 2", vec![p1.id]))?;

        chains::set_chain_favorite(conn, c1.id, true)?;

        // List all.
        let all = chains::list_chains(
            conn,
            &ChainFilter {
                user_id: Some(uid),
                ..Default::default()
            },
        )?;
        assert_eq!(all.len(), 2);

        // List favorites only.
        let favs = chains::list_chains(
            conn,
            &ChainFilter {
                user_id: Some(uid),
                is_favorite: Some(true),
                ..Default::default()
            },
        )?;
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].title, "Chain 1");
        Ok(())
    })
    .unwrap();
}

// ---- Taxonomy Associations ----

#[test]
fn chain_taxonomy_associations() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "taxouser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "X"))?;

        let tag1 = tags::create_tag(conn, uid, "tag-a")?;
        let tag2 = tags::create_tag(conn, uid, "tag-b")?;

        let mut new = make_chain(uid, "Tagged Chain", vec![p1.id]);
        new.tag_ids = vec![tag1.id, tag2.id];
        let chain = chains::create_chain(conn, &new)?;

        let detail = chains::get_chain_with_steps(conn, chain.id)?;
        assert_eq!(detail.tags.len(), 2);

        // Unlink one tag.
        chains::unlink_chain_tag(conn, chain.id, tag1.id)?;
        let tags_after = chains::get_tags_for_chain(conn, chain.id)?;
        assert_eq!(tags_after.len(), 1);
        assert_eq!(tags_after[0].name, "tag-b");

        // Link it back.
        chains::link_chain_tag(conn, chain.id, tag1.id)?;
        let tags_final = chains::get_tags_for_chain(conn, chain.id)?;
        assert_eq!(tags_final.len(), 2);
        Ok(())
    })
    .unwrap();
}

// ---- FTS5 Search ----

#[test]
fn search_chains_by_title() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "searchuser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "X"))?;

        let _c1 = chains::create_chain(
            conn,
            &NewChain {
                title: "Machine Learning Pipeline".to_owned(),
                description: Some("An ML pipeline chain".to_owned()),
                ..make_chain(uid, "", vec![p1.id])
            },
        )?;
        let _c2 = chains::create_chain(
            conn,
            &NewChain {
                title: "Web Development Workflow".to_owned(),
                description: Some("A web dev chain".to_owned()),
                ..make_chain(uid, "", vec![p1.id])
            },
        )?;

        let results = chains::search_chains(conn, uid, "machine", &ChainFilter::default())?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Machine Learning Pipeline");

        let results2 = chains::search_chains(conn, uid, "chain", &ChainFilter::default())?;
        assert_eq!(results2.len(), 2);
        Ok(())
    })
    .unwrap();
}

// ---- Step Count ----

#[test]
fn count_chain_steps() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "countuser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "X"))?;
        let p2 = prompts::create_prompt(conn, &make_prompt(uid, "P2", "Y"))?;

        let chain = chains::create_chain(conn, &make_chain(uid, "Counted", vec![p1.id, p2.id]))?;
        let count = chains::count_steps(conn, chain.id)?;
        assert_eq!(count, 2);
        Ok(())
    })
    .unwrap();
}

// ---- Not Found ----

#[test]
fn get_nonexistent_chain_returns_not_found() {
    let db = setup_db();
    db.with_connection(|conn| {
        let result = chains::get_chain(conn, 99999);
        assert!(result.is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_nonexistent_chain_returns_not_found() {
    let db = setup_db();
    db.with_connection(|conn| {
        let result = chains::delete_chain(conn, 99999);
        assert!(result.is_err());
        Ok(())
    })
    .unwrap();
}
