//! Composition-facing configuration API for the `AsterFSD` server binary.

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

/// Typed file configuration and runtime adapter conversion.
pub mod config;
