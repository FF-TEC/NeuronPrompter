// =============================================================================
// Tests for template variable extraction and substitution (F-26).
//
// Covers variable detection (valid/invalid patterns, edge cases), extraction
// ordering, substitution with partial maps, and content with no variables.
// =============================================================================

use std::collections::HashMap;

use crate::template::{extract_template_variables, substitute_variables};

// ---------------------------------------------------------------------------
// extract_template_variables
// ---------------------------------------------------------------------------

#[test]
fn extract_single_variable() {
    let vars = extract_template_variables("Hello {{name}}!");
    assert_eq!(vars, vec!["name"]);
}

#[test]
fn extract_multiple_variables_preserves_order() {
    let content = "{{greeting}} {{name}}, welcome to {{place}}!";
    let vars = extract_template_variables(content);
    assert_eq!(vars, vec!["greeting", "name", "place"]);
}

#[test]
fn extract_deduplicates_repeated_variables() {
    let content = "{{name}} said hello to {{name}} again.";
    let vars = extract_template_variables(content);
    assert_eq!(vars, vec!["name"]);
}

#[test]
fn extract_preserves_first_appearance_order() {
    let content = "{{b}} {{a}} {{c}} {{a}} {{b}}";
    let vars = extract_template_variables(content);
    assert_eq!(vars, vec!["b", "a", "c"]);
}

#[test]
fn extract_underscored_variable() {
    let vars = extract_template_variables("{{foo_bar}}");
    assert_eq!(vars, vec!["foo_bar"]);
}

#[test]
fn extract_leading_underscore() {
    let vars = extract_template_variables("{{_private}}");
    assert_eq!(vars, vec!["_private"]);
}

#[test]
fn extract_alphanumeric_variable() {
    let vars = extract_template_variables("{{var123}}");
    assert_eq!(vars, vec!["var123"]);
}

#[test]
fn extract_ignores_numeric_start() {
    // Variables starting with digits are not valid identifiers.
    let vars = extract_template_variables("{{123invalid}}");
    assert!(vars.is_empty());
}

#[test]
fn extract_ignores_empty_braces() {
    let vars = extract_template_variables("{{}}");
    assert!(vars.is_empty());
}

#[test]
fn extract_ignores_spaces_inside_braces() {
    let vars = extract_template_variables("{{ name }}");
    assert!(vars.is_empty());
}

#[test]
fn extract_ignores_single_braces() {
    let vars = extract_template_variables("{name}");
    assert!(vars.is_empty());
}

#[test]
fn extract_ignores_triple_braces() {
    // {{{name}}} -- the regex matches {{name}} within the triple braces.
    let vars = extract_template_variables("{{{name}}}");
    assert_eq!(vars, vec!["name"]);
}

#[test]
fn extract_no_variables_in_plain_text() {
    let vars = extract_template_variables("No variables here.");
    assert!(vars.is_empty());
}

#[test]
fn extract_empty_content() {
    let vars = extract_template_variables("");
    assert!(vars.is_empty());
}

#[test]
fn extract_mixed_valid_and_invalid() {
    let content = "{{valid}} {{123bad}} {{also_valid}} {{ spaced }}";
    let vars = extract_template_variables(content);
    assert_eq!(vars, vec!["valid", "also_valid"]);
}

#[test]
fn extract_multiline_content() {
    let content = "Line 1: {{var_a}}\nLine 2: {{var_b}}\nLine 3: {{var_a}}";
    let vars = extract_template_variables(content);
    assert_eq!(vars, vec!["var_a", "var_b"]);
}

#[test]
fn extract_adjacent_variables() {
    let vars = extract_template_variables("{{a}}{{b}}{{c}}");
    assert_eq!(vars, vec!["a", "b", "c"]);
}

#[test]
fn extract_uppercase_variable() {
    let vars = extract_template_variables("{{MyVar}}");
    assert_eq!(vars, vec!["MyVar"]);
}

#[test]
fn extract_ignores_hyphenated_name() {
    // Hyphens are not allowed in variable names.
    let vars = extract_template_variables("{{my-var}}");
    assert!(vars.is_empty());
}

// ---------------------------------------------------------------------------
// substitute_variables
// ---------------------------------------------------------------------------

#[test]
fn substitute_single_variable() {
    let mut values = HashMap::new();
    values.insert("name".to_owned(), "World".to_owned());
    let result = substitute_variables("Hello {{name}}!", &values);
    assert_eq!(result, "Hello World!");
}

#[test]
fn substitute_multiple_variables() {
    let mut values = HashMap::new();
    values.insert("greeting".to_owned(), "Hi".to_owned());
    values.insert("name".to_owned(), "Alice".to_owned());
    let result = substitute_variables("{{greeting}} {{name}}!", &values);
    assert_eq!(result, "Hi Alice!");
}

#[test]
fn substitute_leaves_unmatched_variables() {
    let values = HashMap::new();
    let result = substitute_variables("Hello {{name}}!", &values);
    assert_eq!(result, "Hello {{name}}!");
}

#[test]
fn substitute_partial_map() {
    let mut values = HashMap::new();
    values.insert("a".to_owned(), "X".to_owned());
    let result = substitute_variables("{{a}} and {{b}}", &values);
    assert_eq!(result, "X and {{b}}");
}

#[test]
fn substitute_repeated_variable() {
    let mut values = HashMap::new();
    values.insert("x".to_owned(), "Y".to_owned());
    let result = substitute_variables("{{x}} {{x}} {{x}}", &values);
    assert_eq!(result, "Y Y Y");
}

#[test]
fn substitute_no_variables_in_content() {
    let values = HashMap::new();
    let result = substitute_variables("No variables here.", &values);
    assert_eq!(result, "No variables here.");
}

#[test]
fn substitute_empty_content() {
    let values = HashMap::new();
    let result = substitute_variables("", &values);
    assert_eq!(result, "");
}

#[test]
fn substitute_with_empty_replacement() {
    let mut values = HashMap::new();
    values.insert("name".to_owned(), String::new());
    let result = substitute_variables("Hello {{name}}!", &values);
    assert_eq!(result, "Hello !");
}

#[test]
fn substitute_value_containing_braces() {
    let mut values = HashMap::new();
    values.insert("code".to_owned(), "{{literal}}".to_owned());
    let result = substitute_variables("Output: {{code}}", &values);
    // The replacement contains {{literal}}, which is then part of the output
    // but not re-processed (single-pass substitution).
    assert_eq!(result, "Output: {{literal}}");
}

#[test]
fn substitute_multiline() {
    let mut values = HashMap::new();
    values.insert("header".to_owned(), "Title".to_owned());
    values.insert("body".to_owned(), "Content".to_owned());
    let content = "# {{header}}\n\n{{body}}";
    let result = substitute_variables(content, &values);
    assert_eq!(result, "# Title\n\nContent");
}
