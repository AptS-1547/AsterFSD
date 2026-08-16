use aster_fsd_codec::RawPacket;
use aster_fsd_model::{ClientDataKind, Command, ErrorCode, FlightPlan, HandoffKind, QueryKind};
use aster_fsd_protocol::ProtocolError;

use super::{
    callsign, destination, direct_callsign, direct_destination, parse_i32, require_fields, syntax,
};

pub(super) fn decode_text(packet: &RawPacket) -> Result<Command, ProtocolError> {
    require_fields(packet, 1)?;
    Ok(Command::Text {
        source: callsign(&packet.source, ErrorCode::InvalidSource)?,
        destination: destination(&packet.destination)?,
        message: packet.fields.join(":"),
    })
}

pub(super) fn decode_ping_pong(packet: &RawPacket) -> Result<Command, ProtocolError> {
    let source = callsign(&packet.source, ErrorCode::InvalidSource)?;
    let destination = destination(&packet.destination)?;
    let payload = packet.fields.join(":");
    if packet.command == "PI" {
        Ok(Command::Ping {
            source,
            destination,
            payload,
        })
    } else {
        Ok(Command::Pong {
            source,
            destination,
            payload,
        })
    }
}

pub(super) fn decode_kill(packet: &RawPacket) -> Result<Command, ProtocolError> {
    require_fields(packet, 1)?;
    Ok(Command::Kill {
        source: callsign(&packet.source, ErrorCode::InvalidSource)?,
        target: callsign(&packet.destination, ErrorCode::InvalidCallsign)?,
        reason: packet.fields.join(":"),
    })
}

pub(super) fn decode_flight_plan(packet: &RawPacket) -> Result<Command, ProtocolError> {
    if packet.fields.len() < 15 {
        return Err(syntax("$FP requires seventeen fields"));
    }
    Ok(Command::FlightPlan(FlightPlan {
        callsign: callsign(&packet.source, ErrorCode::InvalidSource)?,
        flight_rules: packet.fields[0]
            .chars()
            .next()
            .ok_or_else(|| syntax("missing flight rules"))?,
        aircraft: packet.fields[1].clone(),
        cruise_speed: parse_i32(&packet.fields[2], "cruise speed")?,
        departure: packet.fields[3].clone(),
        estimated_departure: parse_i32(&packet.fields[4], "estimated departure")?,
        actual_departure: parse_i32(&packet.fields[5], "actual departure")?,
        cruise_altitude: packet.fields[6].clone(),
        destination: packet.fields[7].clone(),
        hours_enroute: parse_i32(&packet.fields[8], "hours enroute")?,
        minutes_enroute: parse_i32(&packet.fields[9], "minutes enroute")?,
        hours_fuel: parse_i32(&packet.fields[10], "hours fuel")?,
        minutes_fuel: parse_i32(&packet.fields[11], "minutes fuel")?,
        alternate: packet.fields[12].clone(),
        remarks: packet.fields[13].clone(),
        route: packet.fields[14].clone(),
    }))
}

pub(super) fn decode_query_response(packet: RawPacket) -> Result<Command, ProtocolError> {
    let response = packet.command == "CR";
    require_fields(&packet, if response { 2 } else { 1 })?;
    let source = callsign(&packet.source, ErrorCode::InvalidSource)?;
    let destination = if response {
        direct_destination(&packet.destination, "$CR")?
    } else {
        destination(&packet.destination)?
    };
    let mut fields = packet.fields.into_iter();
    let kind = fields
        .next()
        .map(|value| query_kind(&value))
        .ok_or_else(|| syntax("query requires a kind"))?;
    let arguments = fields.collect();
    if response {
        Ok(Command::Response {
            source,
            destination,
            kind,
            arguments,
        })
    } else {
        Ok(Command::Query {
            source,
            destination,
            kind,
            arguments,
        })
    }
}

pub(super) fn decode_weather(packet: RawPacket) -> Result<Command, ProtocolError> {
    require_fields(&packet, 1)?;
    let source = callsign(&packet.source, ErrorCode::InvalidSource)?;
    let mut fields = packet.fields.into_iter();
    let first = fields
        .next()
        .ok_or_else(|| syntax("weather request requires a payload"))?;
    if packet.command == "WX" {
        return Ok(Command::WeatherRequest {
            source,
            station: first.to_ascii_uppercase(),
            parsed: true,
        });
    }
    let Some(station) = fields.next() else {
        return Ok(Command::Noop { source });
    };
    if !first.eq_ignore_ascii_case("METAR") {
        return Ok(Command::Noop { source });
    }
    Ok(Command::WeatherRequest {
        source,
        station: station.to_ascii_uppercase(),
        parsed: false,
    })
}

pub(super) fn decode_direct_client_command(packet: RawPacket) -> Result<Command, ProtocolError> {
    if matches!(packet.command.as_str(), "HO" | "HA") {
        require_fields(&packet, 1)?;
        return Ok(Command::Handoff {
            source: callsign(&packet.source, ErrorCode::InvalidSource)?,
            target: direct_callsign(&packet.destination, &packet.command)?,
            kind: if packet.command == "HO" {
                HandoffKind::Request
            } else {
                HandoffKind::Accept
            },
            fields: packet.fields,
        });
    }
    if packet.command == "CI" {
        require_fields(&packet, 1)?;
    }
    let kind = match packet.command.as_str() {
        "SB" => ClientDataKind::SquawkBox,
        "PC" => ClientDataKind::ProController,
        "C?" => ClientDataKind::CommunicationRequest,
        "CI" => ClientDataKind::CommunicationReply,
        _ => return Err(syntax("unsupported client-data command")),
    };
    Ok(Command::ClientData {
        source: callsign(&packet.source, ErrorCode::InvalidSource)?,
        target: direct_callsign(&packet.destination, &packet.command)?,
        kind,
        fields: packet.fields,
    })
}

fn query_kind(value: &str) -> QueryKind {
    match value.to_ascii_uppercase().as_str() {
        "RN" => QueryKind::RealName,
        "FP" => QueryKind::FlightPlan,
        "CAPS" => QueryKind::Capabilities,
        "ATIS" => QueryKind::Atis,
        "INF" => QueryKind::SystemInfo,
        "ACC" => QueryKind::AircraftConfiguration,
        _ => QueryKind::Raw(value.to_string()),
    }
}
