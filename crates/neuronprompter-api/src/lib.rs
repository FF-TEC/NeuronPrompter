//! Axum-based REST API server.
//!
//! This crate provides the REST API handlers, router construction, shared
//! application state, error types, and middleware for the NeuronPrompter
//! HTTP server.

pub mod error;
pub mod handlers;
pub mod middleware;
pub mod router;
pub mod session;
pub mod state;

pub use error::ApiError;
pub use router::{build_router, build_router_dev, build_router_headless, warn_if_network_exposed};
pub use state::AppState;

#[cfg(test)]
mod tests;
