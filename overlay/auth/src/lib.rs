//! llm-wiki-auth: minimal auth kernel for llm-wiki-server.
//!
//! Modules are added incrementally by the implementation plan; this file
//! starts as a stub so the crate compiles before any logic is in place.

pub mod schema;

pub mod password;

pub mod session;

pub mod error;

pub mod store;

pub use error::AuthError;
