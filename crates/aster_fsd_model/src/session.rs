use crate::{Callsign, ClientType};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Protocol revision emitted to classic FSD draft 9 peers.
pub const CLASSIC_PROTOCOL_REVISION: u16 = 9;

/// Protocol revision emitted to VATSIM-compatible FSD peers.
pub const VATSIM_PROTOCOL_REVISION: u16 = 100;

/// Wire dialect selected for a listener and recorded on a client session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolDialect {
    Classic,
    Vatsim,
    AsterV1,
}

/// Stable server-local identifier for one accepted TCP connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConnectionId(pub u64);

impl fmt::Display for ConnectionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Lifecycle phase of a connection in the authoritative network state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    Connected,
    Identified,
    Active,
    Closed,
}

/// VATSIM-style client identification supplied before login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identification {
    pub callsign: Callsign,
    pub client_id: String,
    pub client_name: String,
    pub network_id: Option<String>,
}

/// Protocol-independent login request passed to the authentication port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginCommand {
    pub callsign: Callsign,
    pub client_type: ClientType,
    pub network_id: String,
    pub password: String,
    pub requested_rating: i32,
    pub protocol_revision: u16,
    pub real_name: String,
    pub simulator_type: Option<i32>,
}

/// Public, password-free presence announced to authenticated peers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientPresence {
    pub callsign: Callsign,
    pub client_type: ClientType,
    pub network_id: String,
    pub real_name: String,
    pub rating: i32,
    pub protocol_revision: u16,
    pub simulator_type: Option<i32>,
}
