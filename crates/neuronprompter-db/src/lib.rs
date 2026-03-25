//! SQLite persistence layer.
//!
//! This crate manages the SQLite database via both a single-connection wrapper
//! (for backward compatibility and MCP) and an r2d2 connection pool (for the
//! web server). Runs schema migrations and implements all CRUD repository
//! functions. Every function accepts a `&rusqlite::Connection` to operate within
//! the caller's scope.

pub mod connection;
pub mod connection_provider;
pub mod error;
pub mod migrations;
pub mod pool;
pub mod repo;

pub use connection::Database;
pub use connection_provider::ConnectionProvider;
pub use error::DbError;
pub use pool::{DbPool, PooledConn, create_in_memory_pool, create_pool, create_pool_with_size};

/// Re-exported for use by service-layer functions that accept a connection
/// reference inside `ConnectionProvider` closures.
pub use rusqlite::Connection as SqliteConnection;

/// Re-exported for the database backup functionality.
pub use rusqlite::backup;

#[cfg(test)]
mod tests;
