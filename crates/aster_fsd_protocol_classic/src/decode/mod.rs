mod login;
mod position;
mod routed;

use aster_fsd_codec::{RawPacket, RawPacketKind};
use aster_fsd_model::{Callsign, Command, Destination, ErrorCode, ModelError};
use aster_fsd_protocol::{ProtocolError, ProtocolErrorKind};

pub(super) fn decode(frame: &[u8]) -> Result<Command, ProtocolError> {
    let packet = RawPacket::parse(frame)
        .map_err(|error| ProtocolError::new(ProtocolErrorKind::Syntax, error.to_string()))?;
    tracing::debug!(
        packet_type = ?packet.kind,
        command = %packet.command,
        source = %packet.source,
        destination = %packet.destination,
        fields = packet.fields.len(),
        wire_bytes = frame.len(),
        "Decoded classic packet envelope"
    );
    decode_packet(packet)
}

fn decode_packet(packet: RawPacket) -> Result<Command, ProtocolError> {
    if matches!(
        packet.kind,
        RawPacketKind::PilotPosition | RawPacketKind::AtcPosition
    ) {
        return position::decode(&packet);
    }
    if matches!(packet.command.as_str(), "CQ" | "CR") {
        return routed::decode_query_response(packet);
    }
    if matches!(packet.command.as_str(), "WX" | "AX") {
        return routed::decode_weather(packet);
    }
    if matches!(
        packet.command.as_str(),
        "HO" | "HA" | "SB" | "PC" | "C?" | "CI"
    ) {
        return routed::decode_direct_client_command(packet);
    }
    match packet.command.as_str() {
        "AA" | "AP" => login::decode(&packet),
        "DA" | "DP" => Ok(Command::Logoff {
            source: callsign(&packet.source, ErrorCode::InvalidSource)?,
        }),
        "TM" => routed::decode_text(&packet),
        "FP" => routed::decode_flight_plan(&packet),
        "PI" | "PO" => routed::decode_ping_pong(&packet),
        "!!" => routed::decode_kill(&packet),
        command => Err(ProtocolError::new(
            ProtocolErrorKind::Unsupported,
            format!("unsupported classic command {command}"),
        )),
    }
}

fn syntax(message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(ProtocolErrorKind::Syntax, message)
}

fn field<'a>(
    fields: &'a [String],
    index: usize,
    name: &'static str,
) -> Result<&'a str, ProtocolError> {
    fields
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| syntax(format!("missing {name}")))
}

fn parse_i32(value: &str, name: &'static str) -> Result<i32, ProtocolError> {
    value.parse().map_err(|_| syntax(format!("invalid {name}")))
}

fn parse_protocol_revision(value: &str) -> Result<u16, ProtocolError> {
    value.parse().map_err(|_| {
        ProtocolError::new(
            ProtocolErrorKind::InvalidField,
            "protocol revision is not an unsigned integer",
        )
        .with_error_code(ErrorCode::InvalidProtocolRevision)
    })
}

fn parse_u32(value: &str, name: &'static str) -> Result<u32, ProtocolError> {
    value.parse().map_err(|_| syntax(format!("invalid {name}")))
}

fn parse_f64(value: &str, name: &'static str) -> Result<f64, ProtocolError> {
    value.parse().map_err(|_| syntax(format!("invalid {name}")))
}

fn callsign(value: &str, error_code: ErrorCode) -> Result<Callsign, ProtocolError> {
    Callsign::parse(value).map_err(|error| match error {
        ModelError::InvalidCallsignLength | ModelError::InvalidCallsignCharacter => {
            ProtocolError::new(ProtocolErrorKind::InvalidField, error.to_string())
                .with_error_code(error_code)
        }
        _ => ProtocolError::new(ProtocolErrorKind::Syntax, error.to_string()),
    })
}

fn destination(value: &str) -> Result<Destination, ProtocolError> {
    Destination::parse(value)
        .map_err(|error| ProtocolError::new(ProtocolErrorKind::InvalidField, error.to_string()))
}

fn direct_destination(value: &str, command: &str) -> Result<Destination, ProtocolError> {
    let destination = destination(value)?;
    if matches!(destination, Destination::Direct(_)) {
        return Ok(destination);
    }
    Err(syntax(format!(
        "{command} requires a direct callsign destination"
    )))
}

fn direct_callsign(value: &str, command: &str) -> Result<Callsign, ProtocolError> {
    match direct_destination(value, command)? {
        Destination::Direct(callsign) => Ok(callsign),
        Destination::Server
        | Destination::All
        | Destination::Atc
        | Destination::Pilots
        | Destination::Range(_) => Err(syntax(format!(
            "{command} requires a direct callsign destination"
        ))),
    }
}

fn require_fields(packet: &RawPacket, minimum: usize) -> Result<(), ProtocolError> {
    if packet.fields.len() < minimum {
        return Err(syntax(format!(
            "{} requires at least {minimum} payload field(s)",
            packet.command
        )));
    }
    Ok(())
}
