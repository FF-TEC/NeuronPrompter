//! Service layer for NeuronPrompter business logic.
//!
//! This crate contains the business logic coordinating transactions, version
//! management, association synchronization, import/export workflows, and
//! Ollama LLM integration. Services delegate persistence to the db crate
//! and are invoked by the API handlers.
//!
//! # Architectural note
//!
//! This crate has a direct dependency on `rusqlite` because
//! `ConnectionProvider` closures receive `&rusqlite::Connection`.

pub mod association_sync;
pub mod category_service;
pub mod chain_service;
pub mod collection_service;
pub mod copy_service;
pub mod error;
pub mod io_service;
pub mod ollama;
pub mod prompt_service;
pub mod script_service;
pub mod script_version_service;
pub mod search_service;
pub mod settings_service;
pub mod sync_service;
pub mod tag_service;
pub mod user_service;
pub mod version_service;

pub use error::ServiceError;

#[cfg(test)]
mod tests;
