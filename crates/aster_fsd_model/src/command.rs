use crate::{
    Callsign, ClientDataKind, Destination, FlightPlan, HandoffKind, Identification, LoginCommand,
    Position, QueryKind,
};

/// A validated client command produced by a protocol backend.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Recognized wire command whose original protocol semantics are to do
    /// nothing after session/source validation.
    Noop {
        source: Callsign,
    },
    Identify(Identification),
    Login(LoginCommand),
    Logoff {
        source: Callsign,
    },
    Position(Position),
    Text {
        source: Callsign,
        destination: Destination,
        message: String,
    },
    FlightPlan(FlightPlan),
    Query {
        source: Callsign,
        destination: Destination,
        kind: QueryKind,
        arguments: Vec<String>,
    },
    Response {
        source: Callsign,
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
        source: Callsign,
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
    WeatherRequest {
        source: Callsign,
        station: String,
        parsed: bool,
    },
    Kill {
        source: Callsign,
        target: Callsign,
        reason: String,
    },
}

impl Command {
    /// Returns a stable, credential-free command name for metrics and logs.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Noop { .. } => "noop",
            Self::Identify(_) => "identify",
            Self::Login(_) => "login",
            Self::Logoff { .. } => "logoff",
            Self::Position(_) => "position",
            Self::Text { .. } => "text",
            Self::FlightPlan(_) => "flight_plan",
            Self::Query { .. } => "query",
            Self::Response { .. } => "response",
            Self::Ping { .. } => "ping",
            Self::Pong { .. } => "pong",
            Self::Handoff { .. } => "handoff",
            Self::ClientData { .. } => "client_data",
            Self::WeatherRequest { .. } => "weather_request",
            Self::Kill { .. } => "kill",
        }
    }

    /// Returns the typed routing destination when the command carries one.
    #[must_use]
    pub const fn destination(&self) -> Option<&Destination> {
        match self {
            Self::Text { destination, .. }
            | Self::Query { destination, .. }
            | Self::Response { destination, .. }
            | Self::Ping { destination, .. }
            | Self::Pong { destination, .. } => Some(destination),
            Self::Noop { .. }
            | Self::Identify(_)
            | Self::Login(_)
            | Self::Logoff { .. }
            | Self::Position(_)
            | Self::FlightPlan(_)
            | Self::Handoff { .. }
            | Self::ClientData { .. }
            | Self::WeatherRequest { .. }
            | Self::Kill { .. } => None,
        }
    }

    /// Returns an explicitly addressed callsign carried outside [`Destination`].
    ///
    /// Classic handoff and client-data commands are direct-only in the original
    /// C implementation, so their model stores a [`Callsign`] rather than a
    /// destination variant. Exposing it separately keeps structured logging
    /// useful without rebuilding a temporary destination on the hot path.
    #[must_use]
    pub const fn direct_target(&self) -> Option<&Callsign> {
        match self {
            Self::Handoff { target, .. }
            | Self::ClientData { target, .. }
            | Self::Kill { target, .. } => Some(target),
            Self::Noop { .. }
            | Self::Identify(_)
            | Self::Login(_)
            | Self::Logoff { .. }
            | Self::Position(_)
            | Self::Text { .. }
            | Self::FlightPlan(_)
            | Self::Query { .. }
            | Self::Response { .. }
            | Self::Ping { .. }
            | Self::Pong { .. }
            | Self::WeatherRequest { .. } => None,
        }
    }

    /// Returns the command's claimed source callsign for ownership checks.
    #[must_use]
    pub fn source(&self) -> Option<&Callsign> {
        match self {
            Self::Identify(command) => Some(&command.callsign),
            Self::Login(command) => Some(&command.callsign),
            Self::Noop { source }
            | Self::Logoff { source }
            | Self::Text { source, .. }
            | Self::Query { source, .. }
            | Self::Response { source, .. }
            | Self::Ping { source, .. }
            | Self::Pong { source, .. }
            | Self::Handoff { source, .. }
            | Self::ClientData { source, .. }
            | Self::WeatherRequest { source, .. }
            | Self::Kill { source, .. } => Some(source),
            Self::Position(position) => Some(position.callsign()),
            Self::FlightPlan(plan) => Some(&plan.callsign),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClientType, Event};

    #[test]
    fn observability_names_do_not_include_command_payloads() {
        let command = Command::Login(LoginCommand {
            callsign: Callsign::parse("ECP1").unwrap(),
            client_type: ClientType::Pilot,
            network_id: "CID1".to_string(),
            password: "sentinel-secret".to_string(),
            requested_rating: 1,
            protocol_revision: 9,
            real_name: "Test Pilot".to_string(),
            simulator_type: Some(2),
        });
        assert_eq!(command.kind(), "login");
        assert_eq!(command.destination(), None);
        assert_eq!(command.direct_target(), None);
        assert!(!command.kind().contains("sentinel-secret"));

        let handoff = Command::Handoff {
            source: Callsign::parse("ECP1").unwrap(),
            target: Callsign::parse("ECP2").unwrap(),
            kind: HandoffKind::Request,
            fields: vec!["handoff-data".to_string()],
        };
        assert_eq!(handoff.destination(), None);
        assert_eq!(handoff.direct_target().map(Callsign::as_str), Some("ECP2"));

        let event = Event::Text {
            source: Callsign::parse("ECP1").unwrap(),
            destination: Destination::All,
            message: "sentinel-message".to_string(),
        };
        assert_eq!(event.kind(), "text");
        assert!(!event.kind().contains("sentinel-message"));
    }
}
