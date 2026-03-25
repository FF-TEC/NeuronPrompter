// =============================================================================
// Tests for input validation functions.
//
// Each test group covers valid inputs, boundary conditions, and invalid inputs
// for a single validation function. Assertions verify both success and the
// correct error variant/field on failure.
// =============================================================================

use crate::CoreError;
use crate::domain::chain::ChainStepInput;
use crate::validation::{
    MAX_CONTENT_BYTES, chain_has_duplicate_prompts, validate_chain_steps,
    validate_chain_steps_mixed, validate_content, validate_content_size, validate_language_code,
    validate_ollama_url, validate_script_language, validate_separator, validate_taxonomy_name,
    validate_title, validate_username,
};

// ---------------------------------------------------------------------------
// validate_username
// ---------------------------------------------------------------------------

#[test]
fn username_valid_lowercase() {
    assert!(validate_username("alice").is_ok());
}

#[test]
fn username_valid_with_digits_and_underscores() {
    assert!(validate_username("user_123").is_ok());
}

#[test]
fn username_valid_single_char() {
    assert!(validate_username("a").is_ok());
}

#[test]
fn username_valid_all_digits() {
    assert!(validate_username("42").is_ok());
}

#[test]
fn username_valid_underscore_only() {
    assert!(validate_username("_").is_ok());
}

#[test]
fn username_empty_rejected() {
    let err = validate_username("").unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "username"));
}

#[test]
fn username_uppercase_rejected() {
    let err = validate_username("Alice").unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "username"));
}

#[test]
fn username_spaces_rejected() {
    let err = validate_username("a b").unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "username"));
}

#[test]
fn username_special_chars_rejected() {
    let err = validate_username("user@name").unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "username"));
}

#[test]
fn username_unicode_rejected() {
    let err = validate_username("ünser").unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "username"));
}

#[test]
fn username_hyphen_rejected() {
    let err = validate_username("user-name").unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "username"));
}

// ---------------------------------------------------------------------------
// validate_title
// ---------------------------------------------------------------------------

#[test]
fn title_valid_single_char() {
    assert!(validate_title("X").is_ok());
}

#[test]
fn title_valid_at_200_chars() {
    let title = "a".repeat(200);
    assert!(validate_title(&title).is_ok());
}

#[test]
fn title_empty_rejected() {
    let err = validate_title("").unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "title"));
}

#[test]
fn title_exceeds_200_chars_rejected() {
    let title = "a".repeat(201);
    let err = validate_title(&title).unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "title"));
}

#[test]
fn title_unicode_counts_bytes_not_codepoints() {
    // Multi-byte Unicode characters: each char is 2+ bytes. 100 chars = 200+ bytes.
    let title: String = std::iter::repeat_n('ä', 100).collect();
    // .len() counts bytes (200 for 100 x 'ä'), so this is at the boundary.
    assert!(validate_title(&title).is_ok());
}

#[test]
fn title_whitespace_only_rejected() {
    // Whitespace-only titles are rejected after trimming.
    assert!(validate_title("   ").is_err());
}

// ---------------------------------------------------------------------------
// validate_language_code
// ---------------------------------------------------------------------------

#[test]
fn language_code_none_accepted() {
    assert!(validate_language_code(None).is_ok());
}

#[test]
fn language_code_valid_en() {
    assert!(validate_language_code(Some("en")).is_ok());
}

#[test]
fn language_code_valid_de() {
    assert!(validate_language_code(Some("de")).is_ok());
}

#[test]
fn language_code_three_chars_rejected() {
    let err = validate_language_code(Some("abc")).unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "language"));
}

#[test]
fn language_code_single_char_rejected() {
    let err = validate_language_code(Some("e")).unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "language"));
}

#[test]
fn language_code_uppercase_rejected() {
    let err = validate_language_code(Some("EN")).unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "language"));
}

#[test]
fn language_code_empty_rejected() {
    let err = validate_language_code(Some("")).unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "language"));
}

#[test]
fn language_code_digits_rejected() {
    let err = validate_language_code(Some("12")).unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "language"));
}

#[test]
fn language_code_mixed_case_rejected() {
    let err = validate_language_code(Some("En")).unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "language"));
}

// ---------------------------------------------------------------------------
// validate_content
// ---------------------------------------------------------------------------

#[test]
fn content_valid_non_empty() {
    assert!(validate_content("Hello world").is_ok());
}

#[test]
fn content_valid_single_char() {
    assert!(validate_content("x").is_ok());
}

#[test]
fn content_whitespace_only_rejected() {
    // Whitespace-only content is rejected after trimming.
    assert!(validate_content(" ").is_err());
}

#[test]
fn content_empty_rejected() {
    let err = validate_content("").unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "content"));
}

#[test]
fn content_valid_unicode() {
    assert!(validate_content("日本語テキスト").is_ok());
}

#[test]
fn content_valid_multiline() {
    assert!(validate_content("line one\nline two\nline three").is_ok());
}

// ---------------------------------------------------------------------------
// Bug 1 -- New Prompt Button: content validation
//
// The "New Prompt" button creates a prompt with the default placeholder text
// "Enter your prompt here...". This must be accepted by validate_content,
// while truly empty content must be rejected.
// ---------------------------------------------------------------------------

#[test]
fn content_empty_string_rejected_bug1() {
    let err = validate_content("").unwrap_err();
    assert!(
        matches!(err, CoreError::Validation { ref field, .. } if field == "content"),
        "Empty content must be rejected with a Validation error on field 'content'"
    );
}

#[test]
fn content_default_placeholder_accepted_bug1() {
    // The UI creates new prompts with this default content; it must pass validation.
    assert!(
        validate_content("Enter your prompt here...").is_ok(),
        "Default prompt placeholder text must be accepted as valid content"
    );
}

// ---------------------------------------------------------------------------
// validate_separator
// ---------------------------------------------------------------------------

#[test]
fn separator_valid() {
    assert!(validate_separator("\n\n").is_ok());
    assert!(validate_separator("---").is_ok());
    assert!(validate_separator(" ").is_ok());
    // Exactly 100 chars
    assert!(validate_separator(&"x".repeat(100)).is_ok());
}

#[test]
fn separator_empty_rejected() {
    let err = validate_separator("").unwrap_err();
    assert!(matches!(err, CoreError::Validation { field, .. } if field == "separator"));
}

#[test]
fn separator_over_100_rejected() {
    let err = validate_separator(&"x".repeat(101)).unwrap_err();
    assert!(matches!(err, CoreError::Validation { field, .. } if field == "separator"));
}

// ---------------------------------------------------------------------------
// validate_chain_steps
// ---------------------------------------------------------------------------

#[test]
fn chain_steps_valid() {
    assert!(validate_chain_steps(&[1]).is_ok());
    assert!(validate_chain_steps(&[1, 2, 3]).is_ok());
}

#[test]
fn chain_steps_empty_rejected() {
    let err = validate_chain_steps(&[]).unwrap_err();
    assert!(matches!(err, CoreError::Validation { field, .. } if field == "prompt_ids"));
}

// ---------------------------------------------------------------------------
// chain_has_duplicate_prompts
// ---------------------------------------------------------------------------

#[test]
fn chain_no_duplicates() {
    assert!(!chain_has_duplicate_prompts(&[]));
    assert!(!chain_has_duplicate_prompts(&[1, 2, 3]));
}

#[test]
fn chain_with_duplicates() {
    assert!(chain_has_duplicate_prompts(&[1, 2, 1]));
    assert!(chain_has_duplicate_prompts(&[5, 5]));
}

// ---------------------------------------------------------------------------
// validate_script_language
// ---------------------------------------------------------------------------

#[test]
fn script_language_valid() {
    assert!(validate_script_language("python").is_ok());
    assert!(validate_script_language("type-script").is_ok());
    assert!(validate_script_language("c").is_ok());
    assert!(validate_script_language("go123").is_ok());
    // Exactly 30 chars
    assert!(validate_script_language(&"a".repeat(30)).is_ok());
}

#[test]
fn script_language_empty_rejected() {
    let err = validate_script_language("").unwrap_err();
    assert!(matches!(err, CoreError::Validation { field, .. } if field == "script_language"));
}

#[test]
fn script_language_too_long_rejected() {
    let err = validate_script_language(&"a".repeat(31)).unwrap_err();
    assert!(matches!(err, CoreError::Validation { field, .. } if field == "script_language"));
}

#[test]
fn script_language_invalid_chars_rejected() {
    assert!(validate_script_language("Python").is_err()); // uppercase
    assert!(validate_script_language("type script").is_err()); // space
    assert!(validate_script_language("c++").is_err()); // plus
    assert!(validate_script_language("c_sharp").is_err()); // underscore
}

// ---------------------------------------------------------------------------
// validate_chain_steps_mixed
// ---------------------------------------------------------------------------

#[test]
fn chain_steps_mixed_valid() {
    use crate::domain::chain::StepType;
    let steps = vec![
        ChainStepInput {
            step_type: StepType::Prompt,
            item_id: 1,
        },
        ChainStepInput {
            step_type: StepType::Script,
            item_id: 2,
        },
    ];
    assert!(validate_chain_steps_mixed(&steps).is_ok());
}

#[test]
fn chain_steps_mixed_empty_rejected() {
    let err = validate_chain_steps_mixed(&[]).unwrap_err();
    assert!(matches!(err, CoreError::Validation { field, .. } if field == "steps"));
}

// ---------------------------------------------------------------------------
// validate_content_size
// ---------------------------------------------------------------------------

#[test]
fn content_size_within_limit() {
    assert!(validate_content_size("hello", MAX_CONTENT_BYTES).is_ok());
    assert!(validate_content_size(&"x".repeat(MAX_CONTENT_BYTES), MAX_CONTENT_BYTES).is_ok());
}

#[test]
fn content_size_exceeds_limit() {
    let err =
        validate_content_size(&"x".repeat(MAX_CONTENT_BYTES + 1), MAX_CONTENT_BYTES).unwrap_err();
    assert!(matches!(err, CoreError::Validation { field, .. } if field == "content"));
}

#[test]
fn content_size_custom_limit() {
    assert!(validate_content_size("abc", 3).is_ok());
    assert!(validate_content_size("abcd", 3).is_err());
}

// ---------------------------------------------------------------------------
// validate_taxonomy_name
// ---------------------------------------------------------------------------

#[test]
fn taxonomy_name_valid() {
    assert!(validate_taxonomy_name("coding").is_ok());
    assert!(validate_taxonomy_name("My Tag").is_ok());
    assert!(validate_taxonomy_name(&"a".repeat(100)).is_ok());
}

#[test]
fn taxonomy_name_empty_rejected() {
    assert!(validate_taxonomy_name("").is_err());
}

#[test]
fn taxonomy_name_whitespace_only_rejected() {
    assert!(validate_taxonomy_name("   ").is_err());
    assert!(validate_taxonomy_name("\t\n").is_err());
}

#[test]
fn taxonomy_name_too_long_rejected() {
    let err = validate_taxonomy_name(&"a".repeat(101)).unwrap_err();
    assert!(matches!(err, CoreError::Validation { field, .. } if field == "name"));
}

// ---------------------------------------------------------------------------
// validate_ollama_url
// ---------------------------------------------------------------------------

#[test]
fn ollama_url_localhost_accepted() {
    assert!(validate_ollama_url("http://localhost:11434").is_ok());
    assert!(validate_ollama_url("http://127.0.0.1:11434").is_ok());
    assert!(validate_ollama_url("https://localhost:11434").is_ok());
    assert!(validate_ollama_url("http://localhost").is_ok());
    assert!(validate_ollama_url("http://localhost/api/generate").is_ok());
}

#[test]
fn ollama_url_ipv6_loopback_accepted() {
    assert!(validate_ollama_url("http://[::1]:11434").is_ok());
}

/// Private network IP ranges (RFC 1918) are accepted for LAN deployments
/// where Ollama runs on a separate machine within the local network.
#[test]
fn ollama_url_private_network_accepted() {
    // 10.0.0.0/8
    assert!(validate_ollama_url("http://10.0.0.1:11434").is_ok());
    assert!(validate_ollama_url("http://10.255.255.255:11434").is_ok());
    // 172.16.0.0/12
    assert!(validate_ollama_url("http://172.16.0.1:11434").is_ok());
    assert!(validate_ollama_url("http://172.31.255.255:11434").is_ok());
    // 192.168.0.0/16
    assert!(validate_ollama_url("http://192.168.1.1:11434").is_ok());
    assert!(validate_ollama_url("http://192.168.0.100:11434").is_ok());
}

/// Public internet hosts and non-private IP ranges are rejected to prevent SSRF.
#[test]
fn ollama_url_external_rejected() {
    // Public domain name
    assert!(validate_ollama_url("http://example.com:11434").is_err());
    // AWS metadata endpoint (link-local, not private)
    assert!(validate_ollama_url("http://169.254.169.254/latest/meta-data/").is_err());
    // Public IP address
    assert!(validate_ollama_url("http://8.8.8.8:11434").is_err());
    // 172.x outside the 172.16-31 private range
    assert!(validate_ollama_url("http://172.32.0.1:11434").is_err());
    assert!(validate_ollama_url("http://172.15.0.1:11434").is_err());
}

#[test]
fn ollama_url_non_http_rejected() {
    assert!(validate_ollama_url("ftp://localhost:11434").is_err());
    assert!(validate_ollama_url("file:///etc/passwd").is_err());
}

#[test]
fn ollama_url_no_scheme_rejected() {
    assert!(validate_ollama_url("localhost:11434").is_err());
}

// ---------------------------------------------------------------------------
// Whitespace-only validation (extended)
// ---------------------------------------------------------------------------

#[test]
fn title_whitespace_tabs_newlines_rejected() {
    assert!(validate_title("\t").is_err());
    assert!(validate_title("\n").is_err());
    assert!(validate_title("\t \n").is_err());
}

#[test]
fn content_whitespace_tabs_newlines_rejected() {
    assert!(validate_content("\t").is_err());
    assert!(validate_content("\n").is_err());
    assert!(validate_content("\t \n").is_err());
}

#[test]
fn title_with_leading_trailing_whitespace_accepted() {
    assert!(validate_title(" hello ").is_ok());
}

#[test]
fn content_with_leading_trailing_whitespace_accepted() {
    assert!(validate_content(" some content ").is_ok());
}

// ---------------------------------------------------------------------------
// Username max length
// ---------------------------------------------------------------------------

#[test]
fn username_at_50_chars_accepted() {
    assert!(validate_username(&"a".repeat(50)).is_ok());
}

#[test]
fn username_at_51_chars_rejected() {
    let err = validate_username(&"a".repeat(51)).unwrap_err();
    assert!(matches!(err, CoreError::Validation { field, .. } if field == "username"));
}

#[test]
fn username_300_chars_rejected() {
    assert!(validate_username(&"a".repeat(300)).is_err());
}

// ---------------------------------------------------------------------------
// DEF-012: Script language leading/trailing hyphens
// ---------------------------------------------------------------------------

#[test]
fn script_language_leading_hyphen_rejected() {
    let err = validate_script_language("-python").unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "script_language"));
}

#[test]
fn script_language_trailing_hyphen_rejected() {
    let err = validate_script_language("python-").unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "script_language"));
}

#[test]
fn script_language_only_hyphen_rejected() {
    let err = validate_script_language("-").unwrap_err();
    assert!(matches!(err, CoreError::Validation { ref field, .. } if field == "script_language"));
}

#[test]
fn script_language_middle_hyphen_accepted() {
    assert!(validate_script_language("objective-c").is_ok());
    assert!(validate_script_language("type-script").is_ok());
    assert!(validate_script_language("x86-64-asm").is_ok());
}
