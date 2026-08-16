//! Supervised multi-listener TCP runtime for `AsterFSD` protocol backends.
//!
//! The server owns bounded framing, per-client mailboxes, writer ownership,
//! listener supervision, timeout and shutdown behavior, plus per-dialect frame
//! caching. Command semantics and authoritative client state remain in the core.

#![cfg_attr(
    not(test),
    deny(
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unreachable,
        clippy::unwrap_used
    )
)]

mod backend_registry;
mod challenge;
mod config;
mod connection;
mod error;
mod listener;
mod runtime;
mod server;

pub use backend_registry::BackendRegistry;
pub use config::{ListenerConfig, ServerConfig};
pub use error::ServerError;
pub use server::{BoundServer, Server};

#[cfg(test)]
mod tests;
