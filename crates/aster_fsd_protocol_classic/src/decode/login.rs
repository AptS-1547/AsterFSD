use aster_fsd_codec::RawPacket;
use aster_fsd_model::{ClientType, Command, ErrorCode, LoginCommand};
use aster_fsd_protocol::ProtocolError;

use super::{callsign, field, parse_i32, parse_protocol_revision, syntax};

pub(super) fn decode(packet: &RawPacket) -> Result<Command, ProtocolError> {
    match packet.command.as_str() {
        "AA" => {
            if packet.fields.len() < 5 {
                return Err(syntax("#AA requires seven fields"));
            }
            Ok(Command::Login(LoginCommand {
                callsign: callsign(&packet.source, ErrorCode::InvalidCallsign)?,
                client_type: ClientType::Atc,
                real_name: field(&packet.fields, 0, "real name")?.to_string(),
                network_id: field(&packet.fields, 1, "network ID")?.to_string(),
                password: field(&packet.fields, 2, "password")?.to_string(),
                requested_rating: parse_i32(field(&packet.fields, 3, "rating")?, "rating")?,
                protocol_revision: parse_protocol_revision(field(
                    &packet.fields,
                    4,
                    "protocol revision",
                )?)?,
                simulator_type: None,
            }))
        }
        "AP" => {
            if packet.fields.len() < 6 {
                return Err(syntax("#AP requires eight fields"));
            }
            Ok(Command::Login(LoginCommand {
                callsign: callsign(&packet.source, ErrorCode::InvalidCallsign)?,
                client_type: ClientType::Pilot,
                network_id: field(&packet.fields, 0, "network ID")?.to_string(),
                password: field(&packet.fields, 1, "password")?.to_string(),
                requested_rating: parse_i32(field(&packet.fields, 2, "rating")?, "rating")?,
                protocol_revision: parse_protocol_revision(field(
                    &packet.fields,
                    3,
                    "protocol revision",
                )?)?,
                simulator_type: Some(parse_i32(
                    field(&packet.fields, 4, "simulator type")?,
                    "simulator type",
                )?),
                real_name: field(&packet.fields, 5, "real name")?.to_string(),
            }))
        }
        _ => Err(syntax("unsupported login command")),
    }
}
