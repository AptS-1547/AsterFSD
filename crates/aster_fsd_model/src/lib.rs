//! Protocol-independent domain types shared by every `AsterFSD` adapter.
//!
//! This crate is the semantic boundary between wire protocols and the network
//! core. It owns validated callsigns, typed destinations, commands, events and
//! authoritative presence data. Protocol-specific token layouts and database
//! models deliberately stay outside this crate.

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

mod command;
mod error;
mod event;
mod flight_plan;
mod identity;
mod position;
mod routing;
mod session;
mod weather;

pub use command::Command;
pub use error::{ErrorCode, ModelError};
pub use event::Event;
pub use flight_plan::FlightPlan;
pub use identity::{AtcRating, AuthenticatedIdentity, Callsign, ClientType, PilotRating};
pub use position::{AtcPosition, PilotPosition, Position};
pub use routing::{ClientDataKind, Destination, HandoffKind, QueryKind};
pub use session::{
    CLASSIC_PROTOCOL_REVISION, ClientPresence, ConnectionId, Identification, LoginCommand,
    ProtocolDialect, SessionPhase, VATSIM_PROTOCOL_REVISION,
};
pub use weather::{CloudLayer, TemperatureLayer, WeatherProfile, WindLayer};
