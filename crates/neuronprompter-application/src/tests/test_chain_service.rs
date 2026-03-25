// =============================================================================
// Chain service integration tests.
//
// Verifies chain creation with validation, update with association sync,
// composed content, prompt deletion protection, duplication, and favorites.
// =============================================================================

use neuronprompter_core::domain::chain::{ChainFilter, NewChain, UpdateChain};

use super::{create_test_user, make_prompt, setup_db};
use crate::{chain_service, prompt_service, tag_service};

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

#[test]
fn create_chain_validates_title() {
    let db = setup_db();
    let uid = create_test_user(&db, "chainvaluser");
    let p = prompt_service::create_prompt(&db, &make_prompt(uid, "P1")).unwrap();

    // Empty title should fail.
    let result = chain_service::create_chain(
        &db,
        &NewChain {
            title: String::new(),
            ..make_chain(uid, "", vec![p.id])
        },
    );
    assert!(result.is_err());
}

#[test]
fn create_chain_validates_empty_steps() {
    let db = setup_db();
    let uid = create_test_user(&db, "emptysteps");

    let result = chain_service::create_chain(&db, &make_chain(uid, "No Steps", vec![]));
    assert!(result.is_err());
}

#[test]
fn create_and_get_chain_service() {
    let db = setup_db();
    let uid = create_test_user(&db, "chainsvc");
    let p1 = prompt_service::create_prompt(&db, &make_prompt(uid, "P1")).unwrap();
    let p2 = prompt_service::create_prompt(&db, &make_prompt(uid, "P2")).unwrap();

    let chain =
        chain_service::create_chain(&db, &make_chain(uid, "My Chain", vec![p1.id, p2.id])).unwrap();
    assert_eq!(chain.title, "My Chain");

    let detail = chain_service::get_chain(&db, chain.id).unwrap();
    assert_eq!(detail.steps.len(), 2);
    assert_eq!(detail.chain.title, "My Chain");
}

#[test]
fn update_chain_with_new_steps() {
    let db = setup_db();
    let uid = create_test_user(&db, "updatechain");
    let p1 = prompt_service::create_prompt(&db, &make_prompt(uid, "P1")).unwrap();
    let p2 = prompt_service::create_prompt(&db, &make_prompt(uid, "P2")).unwrap();
    let p3 = prompt_service::create_prompt(&db, &make_prompt(uid, "P3")).unwrap();

    let chain =
        chain_service::create_chain(&db, &make_chain(uid, "Update Me", vec![p1.id])).unwrap();

    let updated = chain_service::update_chain(
        &db,
        &UpdateChain {
            chain_id: chain.id,
            title: Some("Updated Title".to_owned()),
            prompt_ids: Some(vec![p2.id, p3.id, p1.id]),
            ..UpdateChain {
                chain_id: chain.id,
                title: None,
                description: None,
                notes: None,
                language: None,
                separator: None,
                prompt_ids: None,
                steps: None,
                tag_ids: None,
                category_ids: None,
                collection_ids: None,
            }
        },
    )
    .unwrap();

    assert_eq!(updated.title, "Updated Title");
    let detail = chain_service::get_chain(&db, chain.id).unwrap();
    assert_eq!(detail.steps.len(), 3);
    assert_eq!(detail.steps[0].prompt.as_ref().unwrap().title, "P2");
    assert_eq!(detail.steps[1].prompt.as_ref().unwrap().title, "P3");
    assert_eq!(detail.steps[2].prompt.as_ref().unwrap().title, "P1");
}

#[test]
fn update_chain_with_tag_sync() {
    let db = setup_db();
    let uid = create_test_user(&db, "tagsync");
    let p1 = prompt_service::create_prompt(&db, &make_prompt(uid, "P1")).unwrap();
    let tag1 = tag_service::create_tag(&db, uid, "t1").unwrap();
    let tag2 = tag_service::create_tag(&db, uid, "t2").unwrap();

    let mut new = make_chain(uid, "Tag Chain", vec![p1.id]);
    new.tag_ids = vec![tag1.id];
    let chain = chain_service::create_chain(&db, &new).unwrap();

    // Sync tags: remove tag1, add tag2.
    chain_service::update_chain(
        &db,
        &UpdateChain {
            chain_id: chain.id,
            tag_ids: Some(vec![tag2.id]),
            title: None,
            description: None,
            notes: None,
            language: None,
            separator: None,
            prompt_ids: None,
            steps: None,
            category_ids: None,
            collection_ids: None,
        },
    )
    .unwrap();

    let detail = chain_service::get_chain(&db, chain.id).unwrap();
    assert_eq!(detail.tags.len(), 1);
    assert_eq!(detail.tags[0].name, "t2");
}

#[test]
fn composed_content_via_service() {
    let db = setup_db();
    let uid = create_test_user(&db, "composed");
    let p1 = prompt_service::create_prompt(&db, &make_prompt(uid, "P1")).unwrap();
    let p2 = prompt_service::create_prompt(&db, &make_prompt(uid, "P2")).unwrap();

    let chain =
        chain_service::create_chain(&db, &make_chain(uid, "Composed", vec![p1.id, p2.id])).unwrap();
    let content = chain_service::get_composed_content(&db, chain.id).unwrap();
    assert_eq!(content, "Content for P1\n\nContent for P2");
}

#[test]
fn delete_prompt_blocked_by_chain() {
    let db = setup_db();
    let uid = create_test_user(&db, "delblock");
    let p1 = prompt_service::create_prompt(&db, &make_prompt(uid, "P1")).unwrap();
    let _chain =
        chain_service::create_chain(&db, &make_chain(uid, "Blocker", vec![p1.id])).unwrap();

    let result = prompt_service::delete_prompt(&db, p1.id);
    assert!(
        result.is_err(),
        "Prompt deletion should fail when in a chain"
    );

    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("Blocker"),
        "Error should mention chain title"
    );
}

#[test]
fn delete_prompt_succeeds_after_chain_deleted() {
    let db = setup_db();
    let uid = create_test_user(&db, "delafter");
    let p1 = prompt_service::create_prompt(&db, &make_prompt(uid, "P1")).unwrap();
    let chain =
        chain_service::create_chain(&db, &make_chain(uid, "Temp Chain", vec![p1.id])).unwrap();

    // First delete the chain.
    chain_service::delete_chain(&db, chain.id).unwrap();

    // Now prompt deletion should succeed.
    prompt_service::delete_prompt(&db, p1.id).unwrap();
}

#[test]
fn duplicate_chain_service() {
    let db = setup_db();
    let uid = create_test_user(&db, "dupsvc");
    let p1 = prompt_service::create_prompt(&db, &make_prompt(uid, "P1")).unwrap();
    let chain = chain_service::create_chain(&db, &make_chain(uid, "Orig", vec![p1.id])).unwrap();

    let dup = chain_service::duplicate_chain(&db, chain.id).unwrap();
    assert_eq!(dup.title, "Orig (copy)");
    assert_ne!(dup.id, chain.id);
}

#[test]
fn list_chains_service() {
    let db = setup_db();
    let uid = create_test_user(&db, "listsvc");
    let p1 = prompt_service::create_prompt(&db, &make_prompt(uid, "P1")).unwrap();

    chain_service::create_chain(&db, &make_chain(uid, "A", vec![p1.id])).unwrap();
    chain_service::create_chain(&db, &make_chain(uid, "B", vec![p1.id])).unwrap();

    let all = chain_service::list_chains(
        &db,
        &ChainFilter {
            user_id: Some(uid),
            ..Default::default()
        },
    )
    .unwrap()
    .items;
    assert_eq!(all.len(), 2);
}

#[test]
fn toggle_chain_favorite_service() {
    let db = setup_db();
    let uid = create_test_user(&db, "favtogsvc");
    let p1 = prompt_service::create_prompt(&db, &make_prompt(uid, "P1")).unwrap();
    let chain = chain_service::create_chain(&db, &make_chain(uid, "Fav", vec![p1.id])).unwrap();

    chain_service::toggle_chain_favorite(&db, chain.id, true).unwrap();
    let detail = chain_service::get_chain(&db, chain.id).unwrap();
    assert!(detail.chain.is_favorite);
}

#[test]
fn chains_for_prompt_service() {
    let db = setup_db();
    let uid = create_test_user(&db, "forprompt");
    let p1 = prompt_service::create_prompt(&db, &make_prompt(uid, "P1")).unwrap();
    let p2 = prompt_service::create_prompt(&db, &make_prompt(uid, "P2")).unwrap();

    chain_service::create_chain(&db, &make_chain(uid, "A", vec![p1.id, p2.id])).unwrap();
    chain_service::create_chain(&db, &make_chain(uid, "B", vec![p1.id])).unwrap();

    let containing = chain_service::get_chains_for_prompt(&db, p1.id).unwrap();
    assert_eq!(containing.len(), 2);

    let containing_p2 = chain_service::get_chains_for_prompt(&db, p2.id).unwrap();
    assert_eq!(containing_p2.len(), 1);
}
