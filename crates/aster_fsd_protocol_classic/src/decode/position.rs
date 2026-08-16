use aster_fsd_codec::{RawPacket, RawPacketKind};
use aster_fsd_model::{AtcPosition, Command, ErrorCode, PilotPosition, Position};
use aster_fsd_protocol::{ProtocolError, ProtocolErrorKind};

use super::{callsign, parse_f64, parse_i32, parse_u32, syntax};

pub(super) fn decode(packet: &RawPacket) -> Result<Command, ProtocolError> {
    let callsign = callsign(&packet.source, ErrorCode::InvalidSource)?;
    match packet.kind {
        RawPacketKind::PilotPosition => {
            if packet.fields.len() < 8 {
                return Err(syntax("pilot position requires ten fields"));
            }
            Ok(Command::Position(Position::Pilot(PilotPosition {
                callsign,
                mode: packet
                    .command
                    .chars()
                    .next()
                    .ok_or_else(|| syntax("missing mode"))?,
                squawk: canonical_squawk(&packet.fields[0])?,
                rating: parse_i32(&packet.fields[1], "rating")?,
                latitude: parse_f64(&packet.fields[2], "latitude")?,
                longitude: parse_f64(&packet.fields[3], "longitude")?,
                altitude: parse_i32(&packet.fields[4], "altitude")?,
                groundspeed: parse_i32(&packet.fields[5], "groundspeed")?,
                pbh: parse_u32(&packet.fields[6], "PBH")?,
                flags: parse_i32(&packet.fields[7], "flags")?,
            })))
        }
        RawPacketKind::AtcPosition => {
            if packet.fields.len() < 7 {
                return Err(syntax("ATC position requires eight fields"));
            }
            Ok(Command::Position(Position::Atc(AtcPosition {
                callsign,
                frequency: parse_i32(&packet.fields[0], "frequency")?,
                facility_type: parse_i32(&packet.fields[1], "facility type")?,
                visual_range: parse_i32(&packet.fields[2], "visual range")?,
                rating: parse_i32(&packet.fields[3], "rating")?,
                latitude: parse_f64(&packet.fields[4], "latitude")?,
                longitude: parse_f64(&packet.fields[5], "longitude")?,
                altitude: parse_i32(&packet.fields[6], "altitude")?,
            })))
        }
        _ => Err(syntax("not a position packet")),
    }
}

fn canonical_squawk(value: &str) -> Result<String, ProtocolError> {
    if value.is_empty() || value.len() > 4 || !value.bytes().all(|byte| matches!(byte, b'0'..=b'7'))
    {
        return Err(ProtocolError::new(
            ProtocolErrorKind::InvalidField,
            "squawk must contain between one and four octal digits",
        ));
    }
    value
        .parse::<u16>()
        .map(|squawk| squawk.to_string())
        .map_err(|error| ProtocolError::new(ProtocolErrorKind::InvalidField, error.to_string()))
}
