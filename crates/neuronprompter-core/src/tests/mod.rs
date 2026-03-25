// =============================================================================
// Unit tests for the neuronprompter-core crate.
//
// Covers validation rules, error type formatting, domain type serde round-trips,
// and template variable extraction/substitution.
// =============================================================================

// Test code uses expect/unwrap for concise assertions. Panics are the standard
// mechanism for test failure reporting.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod test_domain;
mod test_error;
mod test_template;
mod test_validation;
