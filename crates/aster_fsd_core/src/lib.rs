//! Authoritative protocol-independent `AsterFSD` network core.
//!
//! The core owns connection lifecycle, callsign uniqueness, authentication,
//! position and flight-plan state, and typed routing. It never decodes wire
//! packets or encodes recipient frames, keeping all protocol backends peers.

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

mod config;
mod effects;
mod network;
mod provider;

pub use config::CoreConfig;
pub use effects::{CloseConnection, Delivery, Effects};
pub use network::{Network, RegisterError, SessionSnapshot};
pub use provider::{
    UnavailableWeatherProvider, WeatherLookup, WeatherObservation, WeatherProvider,
    WeatherProviderError,
};
