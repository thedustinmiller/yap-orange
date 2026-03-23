//! yap-core - Core library for the yap-orange note-taking system
//!
//! This crate contains shared functionality used by both the server and CLI:
//! - Data models (Atom, Block, Edge)
//! - Storage trait (`Store`) — backend-agnostic interface
//! - Wiki link parsing and resolution
//! - Content serialization/deserialization
//! - Tree export/import

pub mod bootstrap;
pub mod content;
pub mod error;
pub mod export;
pub mod file_store;
pub mod hash;
pub mod links;
pub mod models;
pub mod seed;
pub mod store;

pub use error::{Error, Result};
pub use store::Store;
