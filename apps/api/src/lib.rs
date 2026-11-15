//! SERK QUORUM API service: a modular monolith.
//!
//! * `routes` — HTTP handlers (application services) per bounded context.
//! * `context` — authentication (sessions, CSRF) and tenant-scoped
//!   authorization (`OrgScope`).
//! * Domain rules live in `quorum-domain`; persistence in `quorum-storage`;
//!   provider adapters in `quorum-providers`; credentials in `quorum-auth`.

pub mod app;
pub mod config;
pub mod context;
pub mod error;
pub mod request_id;
pub mod routes;
pub mod state;
pub mod throttle;

pub use app::router;
pub use config::Config;
pub use state::AppState;
