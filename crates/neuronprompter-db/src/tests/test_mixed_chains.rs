// =============================================================================
// Mixed chain step integration tests.
//
// Verifies polymorphic chain steps that reference both prompts and scripts.
// Covers creation, resolved step retrieval, composed content, duplication,
// script reference lookups, step replacement, and delete protection.
// =============================================================================

use neuronprompter_core::domain::chain::{ChainStepInput, NewChain, StepType};
use neuronprompter_core::domain::prompt::NewPrompt;
use neuronprompter_core::domain::script::NewScript;
use neuronprompter_core::domain::user::NewUser;

use super::setup_db;
use crate::ConnectionProvider;
use crate::repo::{chains, prompts, scripts, users};

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

fn make_script(user_id: i64, title: &str, content: &str, lang: &str) -> NewScript {
    NewScript {
        user_id,
        title: title.to_owned(),
        content: content.to_owned(),
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

fn make_chain_with_steps(user_id: i64, title: &str, steps: Vec<ChainStepInput>) -> NewChain {
    NewChain {
        user_id,
        title: title.to_owned(),
        description: None,
        notes: None,
        language: None,
        separator: None,
        prompt_ids: Vec::new(),
        steps,
        tag_ids: Vec::new(),
        category_ids: Vec::new(),
        collection_ids: Vec::new(),
    }
}

// ---- Create & Resolve ----

#[test]
fn chain_with_mixed_steps() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "mixeduser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "Prompt A", "Prompt content"))?;
        let s1 = scripts::create_script(
            conn,
            &make_script(uid, "Script A", "Script content", "python"),
        )?;

        let steps = vec![
            ChainStepInput {
                step_type: StepType::Prompt,
                item_id: p1.id,
            },
            ChainStepInput {
                step_type: StepType::Script,
                item_id: s1.id,
            },
        ];
        let chain = chains::create_chain(conn, &make_chain_with_steps(uid, "Mixed Chain", steps))?;

        assert_eq!(chain.title, "Mixed Chain");

        let count = chains::count_steps(conn, chain.id)?;
        assert_eq!(count, 2);
        Ok(())
    })
    .unwrap();
}

#[test]
fn get_resolved_mixed_steps() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "resolveduser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "Prompt B", "PC"))?;
        let s1 = scripts::create_script(conn, &make_script(uid, "Script B", "SC", "bash"))?;

        let steps = vec![
            ChainStepInput {
                step_type: StepType::Prompt,
                item_id: p1.id,
            },
            ChainStepInput {
                step_type: StepType::Script,
                item_id: s1.id,
            },
        ];
        let chain =
            chains::create_chain(conn, &make_chain_with_steps(uid, "Resolved Mixed", steps))?;

        let detail = chains::get_chain_with_steps(conn, chain.id)?;
        assert_eq!(detail.steps.len(), 2);

        // First step: prompt.
        assert_eq!(detail.steps[0].step.step_type, StepType::Prompt);
        assert_eq!(detail.steps[0].step.position, 0);
        let prompt = detail.steps[0]
            .prompt
            .as_ref()
            .expect("prompt should be present");
        assert_eq!(prompt.title, "Prompt B");
        assert!(detail.steps[0].script.is_none());

        // Second step: script.
        assert_eq!(detail.steps[1].step.step_type, StepType::Script);
        assert_eq!(detail.steps[1].step.position, 1);
        let script = detail.steps[1]
            .script
            .as_ref()
            .expect("script should be present");
        assert_eq!(script.title, "Script B");
        assert_eq!(script.script_language, "bash");
        assert!(detail.steps[1].prompt.is_none());
        Ok(())
    })
    .unwrap();
}

// ---- Composed Content ----

#[test]
fn composed_content_mixed() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "composedmixed");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "First part"))?;
        let s1 = scripts::create_script(conn, &make_script(uid, "S1", "Second part", "python"))?;
        let p2 = prompts::create_prompt(conn, &make_prompt(uid, "P2", "Third part"))?;

        let steps = vec![
            ChainStepInput {
                step_type: StepType::Prompt,
                item_id: p1.id,
            },
            ChainStepInput {
                step_type: StepType::Script,
                item_id: s1.id,
            },
            ChainStepInput {
                step_type: StepType::Prompt,
                item_id: p2.id,
            },
        ];
        let chain =
            chains::create_chain(conn, &make_chain_with_steps(uid, "Composed Mixed", steps))?;

        let content = chains::get_composed_content(conn, chain.id)?;
        assert_eq!(content, "First part\n\nSecond part\n\nThird part");
        Ok(())
    })
    .unwrap();
}

// ---- Duplicate ----

#[test]
fn duplicate_chain_preserves_step_types() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "dupmixed");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "PA"))?;
        let s1 = scripts::create_script(conn, &make_script(uid, "S1", "SA", "python"))?;

        let steps = vec![
            ChainStepInput {
                step_type: StepType::Prompt,
                item_id: p1.id,
            },
            ChainStepInput {
                step_type: StepType::Script,
                item_id: s1.id,
            },
        ];
        let chain = chains::create_chain(conn, &make_chain_with_steps(uid, "Dup Source", steps))?;

        let dup = chains::duplicate_chain(conn, chain.id)?;
        assert_eq!(dup.title, "Dup Source (copy)");
        assert_ne!(dup.id, chain.id);

        let detail = chains::get_chain_with_steps(conn, dup.id)?;
        assert_eq!(detail.steps.len(), 2);
        assert_eq!(detail.steps[0].step.step_type, StepType::Prompt);
        assert_eq!(detail.steps[0].prompt.as_ref().unwrap().title, "P1");
        assert_eq!(detail.steps[1].step.step_type, StepType::Script);
        assert_eq!(detail.steps[1].script.as_ref().unwrap().title, "S1");
        Ok(())
    })
    .unwrap();
}

// ---- Reference Lookups ----

#[test]
fn get_chains_containing_script() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "scriptrefuser");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "X"))?;
        let s1 = scripts::create_script(conn, &make_script(uid, "S1", "Y", "python"))?;

        let steps_a = vec![
            ChainStepInput {
                step_type: StepType::Prompt,
                item_id: p1.id,
            },
            ChainStepInput {
                step_type: StepType::Script,
                item_id: s1.id,
            },
        ];
        let _ca = chains::create_chain(conn, &make_chain_with_steps(uid, "Chain A", steps_a))?;

        let steps_b = vec![ChainStepInput {
            step_type: StepType::Script,
            item_id: s1.id,
        }];
        let _cb = chains::create_chain(conn, &make_chain_with_steps(uid, "Chain B", steps_b))?;

        // Chain with only prompts should not appear.
        let steps_c = vec![ChainStepInput {
            step_type: StepType::Prompt,
            item_id: p1.id,
        }];
        let _cc = chains::create_chain(conn, &make_chain_with_steps(uid, "Chain C", steps_c))?;

        let containing = chains::get_chains_containing_script(conn, s1.id)?;
        assert_eq!(containing.len(), 2);
        let titles: Vec<&str> = containing.iter().map(|c| c.title.as_str()).collect();
        assert!(titles.contains(&"Chain A"));
        assert!(titles.contains(&"Chain B"));

        let count = chains::count_chains_for_script(conn, s1.id)?;
        assert_eq!(count, 2);
        Ok(())
    })
    .unwrap();
}

// ---- Replace Steps ----

#[test]
fn replace_steps_with_mixed() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "replacemixed");
        let p1 = prompts::create_prompt(conn, &make_prompt(uid, "P1", "A"))?;
        let s1 = scripts::create_script(conn, &make_script(uid, "S1", "B", "python"))?;
        let s2 = scripts::create_script(conn, &make_script(uid, "S2", "C", "bash"))?;

        // Create chain with just prompts initially.
        let steps = vec![ChainStepInput {
            step_type: StepType::Prompt,
            item_id: p1.id,
        }];
        let chain = chains::create_chain(conn, &make_chain_with_steps(uid, "Replace Test", steps))?;

        // Replace with mixed steps.
        let new_steps = vec![
            ChainStepInput {
                step_type: StepType::Script,
                item_id: s2.id,
            },
            ChainStepInput {
                step_type: StepType::Script,
                item_id: s1.id,
            },
            ChainStepInput {
                step_type: StepType::Prompt,
                item_id: p1.id,
            },
        ];
        chains::replace_chain_steps_mixed(conn, chain.id, &new_steps)?;

        let detail = chains::get_chain_with_steps(conn, chain.id)?;
        assert_eq!(detail.steps.len(), 3);
        assert_eq!(detail.steps[0].step.step_type, StepType::Script);
        assert_eq!(detail.steps[0].script.as_ref().unwrap().title, "S2");
        assert_eq!(detail.steps[1].step.step_type, StepType::Script);
        assert_eq!(detail.steps[1].script.as_ref().unwrap().title, "S1");
        assert_eq!(detail.steps[2].step.step_type, StepType::Prompt);
        assert_eq!(detail.steps[2].prompt.as_ref().unwrap().title, "P1");

        let content = chains::get_composed_content(conn, chain.id)?;
        assert_eq!(content, "C\n\nB\n\nA");
        Ok(())
    })
    .unwrap();
}

// ---- Delete Protection ----

#[test]
fn delete_protection_for_scripts() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "delprotect");
        let s1 =
            scripts::create_script(conn, &make_script(uid, "Protected Script", "X", "python"))?;

        let steps = vec![ChainStepInput {
            step_type: StepType::Script,
            item_id: s1.id,
        }];
        let _chain = chains::create_chain(conn, &make_chain_with_steps(uid, "Blocker", steps))?;

        // Attempting to delete the script should fail due to RESTRICT.
        let result = scripts::delete_script(conn, s1.id);
        assert!(
            result.is_err(),
            "Script deletion should be blocked by RESTRICT when referenced by a chain"
        );
        Ok(())
    })
    .unwrap();
}
