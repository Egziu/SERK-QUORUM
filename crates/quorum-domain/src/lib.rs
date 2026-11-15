//! Pure domain model for SERK QUORUM.
//!
//! This crate has no I/O. It defines strongly typed identifiers, the role and
//! permission model, input validation at trust boundaries and the lifecycle
//! rules for projects, documents and decision records. Every rule here is
//! enforced again by the application services before persistence; the
//! database adds constraints and row-level security as defence in depth.

pub mod access;
pub mod connect;
pub mod council;
pub mod decision;
pub mod document;
pub mod error;
pub mod ids;
pub mod project;
pub mod validate;

pub use access::{Permission, Role};
pub use error::DomainError;
