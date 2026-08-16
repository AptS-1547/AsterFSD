//! VATSIM-compatible server-first handshake adapter.
//!
//! The backend owns `$DI`/`$ID` identification and delegates the remaining
//! classic-compatible command surface to the classic backend. Listener choice,
//! rather than packet guessing, determines whether this handshake is active.

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

use aster_fsd_codec::{RawPacket, RawPacketKind};
use aster_fsd_model::{
    Callsign, ClientType, Command, ErrorCode, Event, Identification, VATSIM_PROTOCOL_REVISION,
};
use aster_fsd_protocol::{
    DecodeContext, EncodeContext, HandshakeContext, ProtocolBackend, ProtocolDialect,
    ProtocolError, ProtocolErrorKind, WireFrame,
};
use aster_fsd_protocol_classic::ClassicProtocol;

/// VATSIM handshake adapter with classic command compatibility.
#[derive(Debug, Default)]
pub struct VatsimProtocol {
    classic: ClassicProtocol,
}

impl VatsimProtocol {
    fn frame(packet: &RawPacket) -> Result<WireFrame, ProtocolError> {
        let frame = packet
            .encode(4096)
            .map_err(|error| ProtocolError::new(ProtocolErrorKind::Encoding, error.to_string()))?;
        WireFrame::new(frame)
    }

    fn invalid_identification(field: &'static str, reason: &'static str) -> ProtocolError {
        ProtocolError::new(
            ProtocolErrorKind::InvalidField,
            format!("VATSIM identification field {field} {reason}"),
        )
    }

    fn valid_unique_number(value: &str) -> bool {
        let digits = value
            .strip_prefix('+')
            .or_else(|| value.strip_prefix('-'))
            .unwrap_or(value);
        digits.len() == 9 && digits.bytes().all(|byte| byte.is_ascii_digit())
    }

    fn encode_presence(event: &Event) -> Result<WireFrame, ProtocolError> {
        let Event::ClientAdded { client } = event else {
            return Err(ProtocolError::new(
                ProtocolErrorKind::Encoding,
                "VATSIM presence encoder received an unrelated event",
            ));
        };
        let (command, fields) = match client.client_type {
            ClientType::Atc => (
                "AA",
                vec![
                    client.real_name.clone(),
                    client.network_id.clone(),
                    String::new(),
                    client.rating.to_string(),
                    VATSIM_PROTOCOL_REVISION.to_string(),
                ],
            ),
            ClientType::Pilot | ClientType::Observer => (
                "AP",
                vec![
                    client.network_id.clone(),
                    String::new(),
                    client.rating.to_string(),
                    VATSIM_PROTOCOL_REVISION.to_string(),
                    client.simulator_type.unwrap_or_default().to_string(),
                    client.real_name.clone(),
                ],
            ),
        };
        Self::frame(&RawPacket {
            kind: RawPacketKind::Client,
            command: command.to_string(),
            source: client.callsign.to_string(),
            destination: "SERVER".to_string(),
            fields,
        })
    }
}

impl ProtocolBackend for VatsimProtocol {
    fn dialect(&self) -> ProtocolDialect {
        ProtocolDialect::Vatsim
    }

    fn initial_frames(&self, context: &HandshakeContext) -> Result<Vec<WireFrame>, ProtocolError> {
        let packet = RawPacket {
            kind: RawPacketKind::Request,
            command: "DI".to_string(),
            source: "CLIENT".to_string(),
            destination: "SERVER".to_string(),
            fields: vec!["VATSIM FSD V3.13".to_string(), context.challenge.clone()],
        };
        Ok(vec![Self::frame(&packet)?])
    }

    fn decode(&self, context: &DecodeContext, frame: &[u8]) -> Result<Command, ProtocolError> {
        let packet = RawPacket::parse(frame)
            .map_err(|error| ProtocolError::new(ProtocolErrorKind::Syntax, error.to_string()))?;
        tracing::debug!(
            command = %packet.command,
            source = %packet.source,
            destination = %packet.destination,
            fields = packet.fields.len(),
            wire_bytes = frame.len(),
            "Decoded VATSIM packet envelope"
        );
        if packet.command != "ID" {
            return self.classic.decode(context, frame);
        }
        if !packet.destination.eq_ignore_ascii_case("SERVER") {
            return Err(Self::invalid_identification(
                "destination",
                "must be SERVER",
            ));
        }
        let [
            client_id,
            client_name,
            major,
            minor,
            network_id,
            unique_number,
        ] = packet.fields.as_slice()
        else {
            return Err(ProtocolError::new(
                ProtocolErrorKind::Syntax,
                "$ID requires eight fields",
            ));
        };
        if client_id.len() != 4 || !client_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Self::invalid_identification(
                "client id",
                "must be four hexadecimal characters",
            ));
        }
        if major != "3" || minor != "2" {
            return Err(Self::invalid_identification("version", "must be 3:2"));
        }
        if network_id.is_empty() {
            return Err(Self::invalid_identification(
                "network id",
                "must not be empty",
            ));
        }
        if !Self::valid_unique_number(unique_number) {
            return Err(Self::invalid_identification(
                "unique number",
                "must be a signed nine-digit number",
            ));
        }
        Ok(Command::Identify(Identification {
            callsign: Callsign::parse(&packet.source).map_err(|error| {
                ProtocolError::new(ProtocolErrorKind::InvalidField, error.to_string())
                    .with_error_code(ErrorCode::InvalidCallsign)
            })?,
            client_id: client_id.clone(),
            client_name: client_name.clone(),
            network_id: Some(network_id.clone()),
        }))
    }

    fn encode(
        &self,
        context: &EncodeContext,
        event: &aster_fsd_model::Event,
    ) -> Result<Vec<WireFrame>, ProtocolError> {
        if matches!(event, Event::ClientAdded { .. }) {
            return Ok(vec![Self::encode_presence(event)?]);
        }
        self.classic.encode(context, event)
    }

    fn encoding_is_recipient_specific(&self, event: &aster_fsd_model::Event) -> bool {
        self.classic.encoding_is_recipient_specific(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_fsd_model::{ClientPresence, ConnectionId, SessionPhase};

    fn decode_context() -> DecodeContext {
        DecodeContext {
            connection_id: ConnectionId(1),
            phase: SessionPhase::Connected,
            callsign: None,
            challenge: "challenge".to_string(),
        }
    }

    fn encode_context() -> EncodeContext {
        EncodeContext {
            connection_id: ConnectionId(2),
            recipient: None,
            server_name: "AsterFSD".to_string(),
        }
    }

    #[test]
    fn vatsim_listener_is_explicitly_server_speaks_first() {
        let context = HandshakeContext {
            connection_id: ConnectionId(1),
            peer: "127.0.0.1:1234".parse().unwrap(),
            server_name: "AsterFSD".to_string(),
            server_version: "0.2.0".to_string(),
            challenge: "0123456789abcdef012345".to_string(),
        };
        assert_eq!(
            VatsimProtocol::default().initial_frames(&context).unwrap()[0].as_bytes(),
            b"$DISERVER:CLIENT:VATSIM FSD V3.13:0123456789abcdef012345"
        );
    }

    #[test]
    fn identification_is_decoded_before_classic_login() {
        let command = VatsimProtocol::default()
            .decode(
                &decode_context(),
                b"$IDECP4143:SERVER:48e2:swift:3:2:ECP1547:987654321",
            )
            .unwrap();
        assert!(matches!(
            command,
            Command::Identify(Identification {
                callsign,
                client_id,
                client_name,
                network_id: Some(network_id),
            }) if callsign.as_str() == "ECP4143"
                && client_id == "48e2"
                && client_name == "swift"
                && network_id == "ECP1547"
        ));
    }

    #[test]
    fn identification_rejects_missing_fields_and_invalid_callsign() {
        let missing = VatsimProtocol::default()
            .decode(&decode_context(), b"$IDECP1:SERVER:48e2:swift")
            .unwrap_err();
        assert_eq!(missing.kind, ProtocolErrorKind::Syntax);

        let callsign = VatsimProtocol::default()
            .decode(
                &decode_context(),
                b"$IDBAD CALL:SERVER:48e2:swift:3:2:CID1:987654321",
            )
            .unwrap_err();
        assert_eq!(callsign.error_code, Some(ErrorCode::InvalidCallsign));
    }

    #[test]
    fn identification_validates_every_vatsim_fixed_field() {
        for frame in [
            b"$IDECP1:OTHER:48e2:swift:3:2:CID1:987654321".as_slice(),
            b"$IDECP1:SERVER:xyz1:swift:3:2:CID1:987654321".as_slice(),
            b"$IDECP1:SERVER:48e20:swift:3:2:CID1:987654321".as_slice(),
            b"$IDECP1:SERVER:48e2:swift:4:2:CID1:987654321".as_slice(),
            b"$IDECP1:SERVER:48e2:swift:3:1:CID1:987654321".as_slice(),
            b"$IDECP1:SERVER:48e2:swift:3:2::987654321".as_slice(),
            b"$IDECP1:SERVER:48e2:swift:3:2:CID1:12345678".as_slice(),
            b"$IDECP1:SERVER:48e2:swift:3:2:CID1:12345678x".as_slice(),
            b"$IDECP1:SERVER:48e2:swift:3:2:CID1:987654321:EXTRA".as_slice(),
        ] {
            let error = VatsimProtocol::default()
                .decode(&decode_context(), frame)
                .unwrap_err();
            assert!(matches!(
                error.kind,
                ProtocolErrorKind::InvalidField | ProtocolErrorKind::Syntax
            ));
        }

        for number in ["+987654321", "-987654321"] {
            let frame = format!("$IDECP1:server:48e2:swift:3:2:CID1:{number}");
            assert!(matches!(
                VatsimProtocol::default()
                    .decode(&decode_context(), frame.as_bytes())
                    .unwrap(),
                Command::Identify(_)
            ));
        }
    }

    #[test]
    fn vatsim_presence_always_uses_revision_100() {
        let pilot = Event::ClientAdded {
            client: ClientPresence {
                callsign: Callsign::parse("ECP1").unwrap(),
                client_type: ClientType::Pilot,
                network_id: "CID1".to_string(),
                real_name: "Pilot".to_string(),
                rating: 1,
                protocol_revision: VATSIM_PROTOCOL_REVISION,
                simulator_type: Some(2),
            },
        };
        assert_eq!(
            VatsimProtocol::default()
                .encode(&encode_context(), &pilot)
                .unwrap()[0]
                .as_bytes(),
            b"#APECP1:SERVER:CID1::1:100:2:Pilot"
        );

        let atc = Event::ClientAdded {
            client: ClientPresence {
                callsign: Callsign::parse("ZSPD_TWR").unwrap(),
                client_type: ClientType::Atc,
                network_id: "CID2".to_string(),
                real_name: "Controller".to_string(),
                rating: 5,
                protocol_revision: VATSIM_PROTOCOL_REVISION,
                simulator_type: None,
            },
        };
        assert_eq!(
            VatsimProtocol::default()
                .encode(&encode_context(), &atc)
                .unwrap()[0]
                .as_bytes(),
            b"#AAZSPD_TWR:SERVER:Controller:CID2::5:100"
        );
    }
}
