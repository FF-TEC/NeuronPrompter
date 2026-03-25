//! Domain types, validation, error definitions, and shared traits.
//!
//! This crate contains the foundational types shared across all layers of the
//! NeuronPrompter application. It has zero platform dependencies and defines
//! the canonical domain model, repository trait interfaces, and validation
//! logic referenced by the db, application, mcp, and app crates.

pub mod constants;
pub mod domain;
pub mod error;
pub mod paths;
pub mod serde_helpers;
pub mod template;
pub mod validation;

pub use error::CoreError;

#[cfg(test)]
mod tests;
