// =============================================================================
// Search repository integration tests.
//
// Verifies FTS5 full-text search with prefix matching, BM25 ranking,
// filter application, archive exclusion, and edge cases.
// =============================================================================

use neuronprompter_core::domain::prompt::{NewPrompt, PromptFilter};
use neuronprompter_core::domain::user::NewUser;

use super::setup_db;
use crate::ConnectionProvider;
use crate::repo::{categories, prompts, search, tags, users};

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

#[test]
fn search_finds_matching_title() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "searchuser");
        prompts::create_prompt(
            conn,
            &make_prompt(uid, "Rust Programming Guide", "Learn Rust"),
        )?;
        prompts::create_prompt(conn, &make_prompt(uid, "Python Basics", "Learn Python"))?;

        let filter = PromptFilter::default();
        let results = search::search_prompts(conn, uid, "Rust", &filter)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Rust Programming Guide");
        Ok(())
    })
    .unwrap();
}

#[test]
fn search_finds_matching_content() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "contentuser");
        prompts::create_prompt(
            conn,
            &make_prompt(uid, "Title A", "The quick brown fox jumps"),
        )?;
        prompts::create_prompt(conn, &make_prompt(uid, "Title B", "The lazy dog sleeps"))?;

        let filter = PromptFilter::default();
        let results = search::search_prompts(conn, uid, "fox", &filter)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Title A");
        Ok(())
    })
    .unwrap();
}

#[test]
fn search_prefix_matching() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "prefixuser");
        prompts::create_prompt(
            conn,
            &make_prompt(uid, "Translation Helper", "Translates text"),
        )?;
        prompts::create_prompt(
            conn,
            &make_prompt(uid, "Transport Planner", "Plans transportation"),
        )?;

        let filter = PromptFilter::default();
        // "transl" should match "Translation" and "Translates" but not "Transport".
        let results = search::search_prompts(conn, uid, "transl", &filter)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Translation Helper");
        Ok(())
    })
    .unwrap();
}

#[test]
fn search_excludes_archived_by_default() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "archsearch");
        let p = prompts::create_prompt(
            conn,
            &make_prompt(uid, "Archived Prompt", "searchable content"),
        )?;
        prompts::set_archived(conn, p.id, true)?;

        let filter = PromptFilter::default();
        let results = search::search_prompts(conn, uid, "searchable", &filter)?;
        assert!(
            results.is_empty(),
            "archived prompts should be excluded by default"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn search_includes_archived_when_filter_set() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "archfilter");
        let p = prompts::create_prompt(
            conn,
            &make_prompt(uid, "Archived Findable", "hidden content here"),
        )?;
        prompts::set_archived(conn, p.id, true)?;

        let filter = PromptFilter {
            is_archived: Some(true),
            ..PromptFilter::default()
        };
        let results = search::search_prompts(conn, uid, "hidden", &filter)?;
        assert_eq!(results.len(), 1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn search_with_tag_filter() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "tagsearch");
        let tag = tags::create_tag(conn, uid, "important")?;

        prompts::create_prompt(
            conn,
            &NewPrompt {
                tag_ids: vec![tag.id],
                ..make_prompt(uid, "Tagged Prompt", "keyword here")
            },
        )?;
        prompts::create_prompt(
            conn,
            &make_prompt(uid, "Untagged Prompt", "keyword here too"),
        )?;

        let filter = PromptFilter {
            tag_id: Some(tag.id),
            ..PromptFilter::default()
        };
        let results = search::search_prompts(conn, uid, "keyword", &filter)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Tagged Prompt");
        Ok(())
    })
    .unwrap();
}

#[test]
fn search_with_category_filter() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "catsearch");
        let cat = categories::create_category(conn, uid, "writing")?;

        let p = prompts::create_prompt(conn, &make_prompt(uid, "Writing Prompt", "creative text"))?;
        categories::link_prompt_category(conn, p.id, cat.id)?;

        prompts::create_prompt(conn, &make_prompt(uid, "Other Prompt", "creative other"))?;

        let filter = PromptFilter {
            category_id: Some(cat.id),
            ..PromptFilter::default()
        };
        let results = search::search_prompts(conn, uid, "creative", &filter)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Writing Prompt");
        Ok(())
    })
    .unwrap();
}

#[test]
fn search_empty_query_returns_empty() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "emptyquery");
        prompts::create_prompt(conn, &make_prompt(uid, "Prompt", "content"))?;

        let filter = PromptFilter::default();
        let results = search::search_prompts(conn, uid, "", &filter)?;
        assert!(results.is_empty());

        let results2 = search::search_prompts(conn, uid, "   ", &filter)?;
        assert!(results2.is_empty());
        Ok(())
    })
    .unwrap();
}

#[test]
fn search_scoped_to_user() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid1 = create_user(conn, "scopeuser_a");
        let uid2 = create_user(conn, "scopeuser_b");

        prompts::create_prompt(conn, &make_prompt(uid1, "User1 Findable", "unique_term"))?;
        prompts::create_prompt(conn, &make_prompt(uid2, "User2 Also Has", "unique_term"))?;

        let filter = PromptFilter::default();
        let results = search::search_prompts(conn, uid1, "unique_term", &filter)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].user_id, uid1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn search_fts5_special_characters_escaped() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "specialuser");
        prompts::create_prompt(conn, &make_prompt(uid, "C++ Guide", "content about C++"))?;

        let filter = PromptFilter::default();
        // Searching for "C++" should not crash FTS5 parser due to quoting.
        let results = search::search_prompts(conn, uid, "C++", &filter)?;
        // May or may not match depending on tokenization, but should not panic.
        let _ = results;
        Ok(())
    })
    .unwrap();
}

#[test]
fn search_unicode_content() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "unicodeuser");
        prompts::create_prompt(
            conn,
            &make_prompt(uid, "Deutsche Anleitung", "Erstelle eine Zusammenfassung"),
        )?;

        let filter = PromptFilter::default();
        let results = search::search_prompts(conn, uid, "Zusammenfassung", &filter)?;
        assert_eq!(results.len(), 1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn unicode_cjk_and_emoji_round_trip() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "unicodecjk");

        // Chinese
        let tag_zh = tags::create_tag(conn, uid, "\u{4e2d}\u{6587}\u{6807}\u{7b7e}")?;
        assert_eq!(tag_zh.name, "\u{4e2d}\u{6587}\u{6807}\u{7b7e}");

        // Japanese
        let tag_ja = tags::create_tag(conn, uid, "\u{65e5}\u{672c}\u{8a9e}")?;
        assert_eq!(tag_ja.name, "\u{65e5}\u{672c}\u{8a9e}");

        // Arabic
        let tag_ar = tags::create_tag(conn, uid, "\u{0639}\u{0631}\u{0628}\u{064a}")?;
        assert_eq!(tag_ar.name, "\u{0639}\u{0631}\u{0628}\u{064a}");

        // Emoji
        let tag_emoji = tags::create_tag(conn, uid, "\u{1f680}\u{1f4a1}\u{2728}")?;
        assert_eq!(tag_emoji.name, "\u{1f680}\u{1f4a1}\u{2728}");

        // Verify read-back from listing
        let all_tags = tags::list_tags_for_user(conn, uid)?;
        let names: Vec<&str> = all_tags.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"\u{4e2d}\u{6587}\u{6807}\u{7b7e}"));
        assert!(names.contains(&"\u{65e5}\u{672c}\u{8a9e}"));
        assert!(names.contains(&"\u{0639}\u{0631}\u{0628}\u{064a}"));
        assert!(names.contains(&"\u{1f680}\u{1f4a1}\u{2728}"));

        Ok(())
    })
    .unwrap();
}

#[test]
fn search_multi_word_query() {
    let db = setup_db();
    db.with_connection(|conn| {
        let uid = create_user(conn, "multiword");
        prompts::create_prompt(
            conn,
            &make_prompt(uid, "Machine Learning Guide", "deep learning algorithms"),
        )?;
        prompts::create_prompt(
            conn,
            &make_prompt(uid, "Simple Calculator", "basic math operations"),
        )?;

        let filter = PromptFilter::default();
        // Both words must match (implicit AND in FTS5).
        let results = search::search_prompts(conn, uid, "deep learning", &filter)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Machine Learning Guide");
        Ok(())
    })
    .unwrap();
}
