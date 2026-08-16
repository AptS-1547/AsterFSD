//! Versioned Aster-native JSON protocol backend.
//!
//! The adapter gives native clients an explicit schema while preserving the
//! same command, authorization and routing semantics as classic and VATSIM
//! peers. Password fields exist only in the inbound login DTO.

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

use aster_fsd_model::{
    Callsign, ClientDataKind, ClientType, Command, Destination, Event, FlightPlan, HandoffKind,
    Identification, LoginCommand, Position, QueryKind,
};
use aster_fsd_protocol::{
    DecodeContext, EncodeContext, HandshakeContext, ProtocolBackend, ProtocolDialect,
    ProtocolError, ProtocolErrorKind, WireFrame,
};
use serde::Deserialize;
use serde_json::{Value, json};

const VERSION: u16 = 1;

/// Stateless backend for version 1 of the Aster JSON protocol.
#[derive(Debug, Default)]
pub struct AsterProtocolV1;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Incoming {
    Identify {
        v: u16,
        callsign: String,
        client_id: String,
        client_name: String,
        network_id: Option<String>,
    },
    Login {
        v: u16,
        callsign: String,
        client_type: ClientType,
        network_id: String,
        password: String,
        requested_rating: i32,
        real_name: String,
        simulator_type: Option<i32>,
    },
    Logoff {
        v: u16,
        source: String,
    },
    Position {
        v: u16,
        position: Position,
    },
    Text {
        v: u16,
        source: String,
        destination: Destination,
        message: String,
    },
    FlightPlan {
        v: u16,
        plan: FlightPlan,
    },
    Query {
        v: u16,
        source: String,
        destination: Destination,
        kind: QueryKind,
        #[serde(default)]
        arguments: Vec<String>,
    },
    Response {
        v: u16,
        source: String,
        destination: Destination,
        kind: QueryKind,
        #[serde(default)]
        arguments: Vec<String>,
    },
    Ping {
        v: u16,
        source: String,
        destination: Destination,
        #[serde(default)]
        payload: String,
    },
    Handoff {
        v: u16,
        source: String,
        target: String,
        kind: HandoffKind,
        #[serde(default)]
        fields: Vec<String>,
    },
    ClientData {
        v: u16,
        source: String,
        target: String,
        kind: ClientDataKind,
        #[serde(default)]
        fields: Vec<String>,
    },
    WeatherRequest {
        v: u16,
        source: String,
        station: String,
        #[serde(default)]
        parsed: bool,
    },
    Kill {
        v: u16,
        source: String,
        target: String,
        reason: String,
    },
}

impl Incoming {
    fn version(&self) -> u16 {
        match self {
            Self::Identify { v, .. }
            | Self::Login { v, .. }
            | Self::Logoff { v, .. }
            | Self::Position { v, .. }
            | Self::Text { v, .. }
            | Self::FlightPlan { v, .. }
            | Self::Query { v, .. }
            | Self::Response { v, .. }
            | Self::Ping { v, .. }
            | Self::Handoff { v, .. }
            | Self::ClientData { v, .. }
            | Self::WeatherRequest { v, .. }
            | Self::Kill { v, .. } => *v,
        }
    }

    fn callsign(value: String) -> Result<Callsign, ProtocolError> {
        Callsign::parse(value)
            .map_err(|error| ProtocolError::new(ProtocolErrorKind::InvalidField, error.to_string()))
    }

    fn into_routed_command(self) -> Result<Command, ProtocolError> {
        Ok(match self {
            Self::Query {
                source,
                destination,
                kind,
                arguments,
                ..
            } => Command::Query {
                source: Self::callsign(source)?,
                destination,
                kind,
                arguments,
            },
            Self::Response {
                source,
                destination,
                kind,
                arguments,
                ..
            } => Command::Response {
                source: Self::callsign(source)?,
                destination,
                kind,
                arguments,
            },
            Self::Ping {
                source,
                destination,
                payload,
                ..
            } => Command::Ping {
                source: Self::callsign(source)?,
                destination,
                payload,
            },
            Self::Handoff {
                source,
                target,
                kind,
                fields,
                ..
            } => Command::Handoff {
                source: Self::callsign(source)?,
                target: Self::callsign(target)?,
                kind,
                fields,
            },
            Self::ClientData {
                source,
                target,
                kind,
                fields,
                ..
            } => Command::ClientData {
                source: Self::callsign(source)?,
                target: Self::callsign(target)?,
                kind,
                fields,
            },
            _ => {
                return Err(ProtocolError::new(
                    ProtocolErrorKind::Syntax,
                    "internal Aster command classification mismatch",
                ));
            }
        })
    }

    fn into_command(self) -> Result<Command, ProtocolError> {
        if self.version() != VERSION {
            return Err(ProtocolError::new(
                ProtocolErrorKind::Version,
                "unsupported Aster protocol version",
            ));
        }
        Ok(match self {
            Self::Identify {
                callsign,
                client_id,
                client_name,
                network_id,
                ..
            } => Command::Identify(Identification {
                callsign: Self::callsign(callsign)?,
                client_id,
                client_name,
                network_id,
            }),
            Self::Login {
                callsign,
                client_type,
                network_id,
                password,
                requested_rating,
                real_name,
                simulator_type,
                ..
            } => Command::Login(LoginCommand {
                callsign: Self::callsign(callsign)?,
                client_type,
                network_id,
                password,
                requested_rating,
                protocol_revision: VERSION,
                real_name,
                simulator_type,
            }),
            Self::Logoff { source, .. } => Command::Logoff {
                source: Self::callsign(source)?,
            },
            Self::Position { position, .. } => Command::Position(position),
            Self::Text {
                source,
                destination,
                message,
                ..
            } => Command::Text {
                source: Self::callsign(source)?,
                destination,
                message,
            },
            Self::FlightPlan { plan, .. } => Command::FlightPlan(plan),
            command @ (Self::Query { .. }
            | Self::Response { .. }
            | Self::Ping { .. }
            | Self::Handoff { .. }
            | Self::ClientData { .. }) => command.into_routed_command()?,
            Self::WeatherRequest {
                source,
                station,
                parsed,
                ..
            } => Command::WeatherRequest {
                source: Self::callsign(source)?,
                station,
                parsed,
            },
            Self::Kill {
                source,
                target,
                reason,
                ..
            } => Command::Kill {
                source: Self::callsign(source)?,
                target: Self::callsign(target)?,
                reason,
            },
        })
    }
}

impl ProtocolBackend for AsterProtocolV1 {
    fn dialect(&self) -> ProtocolDialect {
        ProtocolDialect::AsterV1
    }

    fn initial_frames(&self, context: &HandshakeContext) -> Result<Vec<WireFrame>, ProtocolError> {
        let value = json!({
            "v": VERSION,
            "type": "hello",
            "server": context.server_name,
            "server_version": context.server_version,
            "connection_id": context.connection_id,
        });
        WireFrame::new(
            serde_json::to_vec(&value).map_err(|error| {
                ProtocolError::new(ProtocolErrorKind::Encoding, error.to_string())
            })?,
        )
        .map(|frame| vec![frame])
    }

    fn decode(&self, _context: &DecodeContext, frame: &[u8]) -> Result<Command, ProtocolError> {
        serde_json::from_slice::<Incoming>(frame)
            .map_err(|error| ProtocolError::new(ProtocolErrorKind::Syntax, error.to_string()))?
            .into_command()
    }

    fn encode(
        &self,
        _context: &EncodeContext,
        event: &Event,
    ) -> Result<Vec<WireFrame>, ProtocolError> {
        let value = serde_json::to_value(event)
            .map_err(|error| ProtocolError::new(ProtocolErrorKind::Encoding, error.to_string()))?;
        let Value::Object(mut object) = value else {
            return Err(ProtocolError::new(
                ProtocolErrorKind::Encoding,
                "event did not serialize as an object",
            ));
        };
        object.insert("v".to_string(), Value::from(VERSION));
        let frame = serde_json::to_vec(&Value::Object(object))
            .map_err(|error| ProtocolError::new(ProtocolErrorKind::Encoding, error.to_string()))?;
        Ok(vec![WireFrame::new(frame)?])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_fsd_model::{ClientDataKind, ConnectionId, HandoffKind, SessionPhase};

    fn decode_context() -> DecodeContext {
        DecodeContext {
            connection_id: ConnectionId(1),
            phase: SessionPhase::Active,
            callsign: Some(Callsign::parse("ECP1").unwrap()),
            challenge: String::new(),
        }
    }

    fn encode_context() -> EncodeContext {
        EncodeContext {
            connection_id: ConnectionId(1),
            recipient: None,
            server_name: "AsterFSD".to_string(),
        }
    }

    #[test]
    fn aster_v1_login_decodes_to_shared_command() {
        let context = DecodeContext {
            connection_id: ConnectionId(1),
            phase: SessionPhase::Connected,
            callsign: None,
            challenge: String::new(),
        };
        let frame = br#"{"v":1,"type":"login","callsign":"ECP4143","client_type":"pilot","network_id":"ECP1547","password":"secret","requested_rating":1,"real_name":"Test Pilot"}"#;
        assert!(matches!(
            AsterProtocolV1.decode(&context, frame).unwrap(),
            Command::Login(_)
        ));
    }

    #[test]
    fn output_events_are_versioned() {
        let context = EncodeContext {
            connection_id: ConnectionId(1),
            recipient: None,
            server_name: "AsterFSD".to_string(),
        };
        let event = Event::WindDelta {
            speed: 2,
            direction: -3,
        };
        let value: Value =
            serde_json::from_slice(AsterProtocolV1.encode(&context, &event).unwrap()[0].as_bytes())
                .unwrap();
        assert_eq!(value["v"], 1);
        assert_eq!(value["type"], "wind_delta");
    }

    #[test]
    fn invalid_json_version_and_callsign_are_separate_boundaries() {
        let context = DecodeContext {
            connection_id: ConnectionId(1),
            phase: SessionPhase::Connected,
            callsign: None,
            challenge: String::new(),
        };
        let syntax = AsterProtocolV1.decode(&context, b"{not-json").unwrap_err();
        assert_eq!(syntax.kind, ProtocolErrorKind::Syntax);

        let version = AsterProtocolV1
            .decode(&context, br#"{"v":2,"type":"logoff","source":"ECP1"}"#)
            .unwrap_err();
        assert_eq!(version.kind, ProtocolErrorKind::Version);

        let callsign = AsterProtocolV1
            .decode(&context, br#"{"v":1,"type":"logoff","source":"BAD CALL"}"#)
            .unwrap_err();
        assert_eq!(callsign.kind, ProtocolErrorKind::InvalidField);
    }

    #[test]
    fn serialized_events_never_grow_a_password_field() {
        let context = EncodeContext {
            connection_id: ConnectionId(1),
            recipient: None,
            server_name: "AsterFSD".to_string(),
        };
        let event = Event::ClientAdded {
            client: aster_fsd_model::ClientPresence {
                callsign: Callsign::parse("ECP1").unwrap(),
                client_type: ClientType::Pilot,
                network_id: "CID1".to_string(),
                real_name: "Test Pilot".to_string(),
                rating: 1,
                protocol_revision: 1,
                simulator_type: Some(2),
            },
        };
        let frame = AsterProtocolV1.encode(&context, &event).unwrap();
        let text = std::str::from_utf8(frame[0].as_bytes()).unwrap();
        assert!(!text.contains("password"));
    }

    #[test]
    fn typed_handoff_and_client_data_round_trip_through_aster_v1() {
        let handoff = AsterProtocolV1
            .decode(
                &decode_context(),
                br#"{"v":1,"type":"handoff","source":"ECP1","target":"ECP2","kind":"request","fields":["ECP3","123.450"]}"#,
            )
            .unwrap();
        assert!(matches!(
            handoff,
            Command::Handoff {
                source,
                target,
                kind: HandoffKind::Request,
                fields,
            } if source.as_str() == "ECP1"
                && target.as_str() == "ECP2"
                && fields == ["ECP3", "123.450"]
        ));

        let client_data = AsterProtocolV1
            .decode(
                &decode_context(),
                br#"{"v":1,"type":"client_data","source":"ECP2","target":"ECP1","kind":"communication_reply","fields":["123.450"]}"#,
            )
            .unwrap();
        assert!(matches!(
            client_data,
            Command::ClientData {
                source,
                target,
                kind: ClientDataKind::CommunicationReply,
                fields,
            } if source.as_str() == "ECP2"
                && target.as_str() == "ECP1"
                && fields == ["123.450"]
        ));

        for event in [
            Event::Handoff {
                source: Callsign::parse("ECP1").unwrap(),
                target: Callsign::parse("ECP2").unwrap(),
                kind: HandoffKind::Accept,
                fields: vec!["ECP3".to_string()],
            },
            Event::ClientData {
                source: Callsign::parse("ECP1").unwrap(),
                target: Callsign::parse("ECP2").unwrap(),
                kind: ClientDataKind::SquawkBox,
                fields: Vec::new(),
            },
        ] {
            let frames = AsterProtocolV1.encode(&encode_context(), &event).unwrap();
            let value: Value = serde_json::from_slice(frames[0].as_bytes()).unwrap();
            assert_eq!(value["v"], VERSION);
            assert_eq!(value["source"], "ECP1");
            assert_eq!(value["target"], "ECP2");
            assert!(matches!(
                value["type"].as_str(),
                Some("handoff" | "client_data")
            ));
        }
    }
}
