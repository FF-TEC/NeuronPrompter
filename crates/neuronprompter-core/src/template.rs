// =============================================================================
// Template variable extraction and substitution (F-26).
//
// Prompts may contain placeholder variables in the format {{variable_name}}.
// This module provides functions to extract unique variable names from prompt
// content and to substitute them with user-supplied values.
// =============================================================================

use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// Compiled regex matching template variable placeholders of the form
/// {{identifier}}. Identifiers must start with a letter or underscore,
/// followed by zero or more alphanumeric characters or underscores.
#[allow(clippy::expect_used)] // Regex pattern is a compile-time constant.
static TEMPLATE_VAR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}").expect("compile-time constant regex")
});

/// Scans prompt content for `{{variable_name}}` placeholders and returns the
/// unique variable names in order of first appearance.
///
/// # Arguments
///
/// * `content` - The prompt text to scan for template variables.
///
/// # Returns
///
/// A vector of unique variable names, preserving the order in which each
/// variable first appears in the content.
pub fn extract_template_variables(content: &str) -> Vec<String> {
    let mut seen_set = HashSet::new();
    let mut seen = Vec::new();
    for cap in TEMPLATE_VAR_RE.captures_iter(content) {
        if let Some(name) = cap.get(1) {
            let name_str = name.as_str().to_owned();
            if seen_set.insert(name_str.clone()) {
                seen.push(name_str);
            }
        }
    }
    seen
}

/// Replaces all `{{variable_name}}` occurrences in the content with the
/// corresponding values from the provided map. Variables without a matching
/// entry in the map are left unchanged in the output.
///
/// # Arguments
///
/// * `content` - The prompt text containing template variables.
/// * `values` - A map from variable name to replacement value.
///
/// # Returns
///
/// A new string with all matched variables substituted.
pub fn substitute_variables<S: std::hash::BuildHasher>(
    content: &str,
    values: &HashMap<String, String, S>,
) -> String {
    TEMPLATE_VAR_RE
        .replace_all(content, |caps: &regex::Captures<'_>| {
            let var_name = &caps[1];
            match values.get(var_name) {
                Some(val) => val.clone(),
                None => caps[0].to_owned(),
            }
        })
        .into_owned()
}
