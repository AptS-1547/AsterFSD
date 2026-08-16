use crate::{
    Callsign, ClientDataKind, ClientPresence, ClientType, Destination, ErrorCode, FlightPlan,
    HandoffKind, Position, QueryKind, WeatherProfile,
};
use serde::{Deserialize, Serialize};

/// Protocol-independent result emitted by the network core.
///
/// Backends encode an event for the recipient's dialect. The enum stays inline
/// in the core's `Delivery` value so hot-path dispatch does not add a heap
/// allocation merely to reduce enum size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Error {
        callsign: Option<Callsign>,
        code: ErrorCode,
        environment: String,
        description: String,
    },
    Welcome {
        callsign: Callsign,
        message: String,
    },
    ClientAdded {
        client: ClientPresence,
    },
    ClientRemoved {
        callsign: Callsign,
        client_type: ClientType,
        network_id: String,
    },
    Position {
        position: Position,
    },
    Text {
        source: Callsign,
        destination: Destination,
        message: String,
    },
    FlightPlan {
        plan: FlightPlan,
        destination: Destination,
    },
    Query {
        /// Callsign or protocol-owned endpoint such as `SERVER`.
        source: String,
        destination: Destination,
        kind: QueryKind,
        arguments: Vec<String>,
    },
    Response {
        /// Callsign or protocol-owned endpoint such as `SERVER`.
        source: String,
        destination: Destination,
        kind: QueryKind,
        arguments: Vec<String>,
    },
    Ping {
        source: Callsign,
        destination: Destination,
        payload: String,
    },
    Pong {
        source: String,
        destination: Destination,
        payload: String,
    },
    Handoff {
        source: Callsign,
        target: Callsign,
        kind: HandoffKind,
        fields: Vec<String>,
    },
    ClientData {
        source: Callsign,
        target: Callsign,
        kind: ClientDataKind,
        fields: Vec<String>,
    },
    WeatherReport {
        source: String,
        destination: Callsign,
        station: String,
        report: String,
    },
    WeatherProfile {
        destination: Callsign,
        station: String,
        profile: WeatherProfile,
    },
    WindDelta {
        speed: i32,
        direction: i32,
    },
    Disconnect {
        target: Callsign,
        reason: String,
    },
}

impl Event {
    /// Returns a stable event name that contains no payload or credentials.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Error { .. } => "error",
            Self::Welcome { .. } => "welcome",
            Self::ClientAdded { .. } => "client_added",
            Self::ClientRemoved { .. } => "client_removed",
            Self::Position { .. } => "position",
            Self::Text { .. } => "text",
            Self::FlightPlan { .. } => "flight_plan",
            Self::Query { .. } => "query",
            Self::Response { .. } => "response",
            Self::Ping { .. } => "ping",
            Self::Pong { .. } => "pong",
            Self::Handoff { .. } => "handoff",
            Self::ClientData { .. } => "client_data",
            Self::WeatherReport { .. } => "weather_report",
            Self::WeatherProfile { .. } => "weather_profile",
            Self::WindDelta { .. } => "wind_delta",
            Self::Disconnect { .. } => "disconnect",
        }
    }
}
