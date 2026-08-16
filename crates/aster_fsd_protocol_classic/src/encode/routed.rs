use aster_fsd_codec::RawPacketKind;
use aster_fsd_model::{Callsign, ClientDataKind, Event, HandoffKind, QueryKind};
use aster_fsd_protocol::{EncodeContext, ProtocolError, WireFrame};

use super::{command_frame, destination_text, event_mismatch};

pub(super) fn encode_query(event: &Event) -> Result<WireFrame, ProtocolError> {
    let (source, destination, kind, arguments, command) = match event {
        Event::Query {
            source,
            destination,
            kind,
            arguments,
        } => (source, destination, kind, arguments, "CQ"),
        Event::Response {
            source,
            destination,
            kind,
            arguments,
        } => (source, destination, kind, arguments, "CR"),
        _ => return Err(event_mismatch("query")),
    };
    let mut fields = vec![match kind {
        QueryKind::RealName => "RN".to_string(),
        QueryKind::FlightPlan => "FP".to_string(),
        QueryKind::Capabilities => "CAPS".to_string(),
        QueryKind::Atis => "ATIS".to_string(),
        QueryKind::SystemInfo => "INF".to_string(),
        QueryKind::AircraftConfiguration => "ACC".to_string(),
        QueryKind::Raw(value) => value.clone(),
    }];
    fields.extend(arguments.clone());
    command_frame(
        RawPacketKind::Request,
        command,
        source.clone(),
        destination_text(destination),
        fields,
    )
}

pub(super) fn encode(_context: &EncodeContext, event: &Event) -> Result<WireFrame, ProtocolError> {
    match event {
        Event::Text {
            source,
            destination,
            message,
        } => command_frame(
            RawPacketKind::Client,
            "TM",
            source.to_string(),
            destination_text(destination),
            vec![message.clone()],
        ),
        Event::Ping {
            source,
            destination,
            payload,
        } => command_frame(
            RawPacketKind::Request,
            "PI",
            source.to_string(),
            destination_text(destination),
            vec![payload.clone()],
        ),
        Event::Pong {
            source,
            destination,
            payload,
        } => command_frame(
            RawPacketKind::Request,
            "PO",
            source.clone(),
            destination_text(destination),
            vec![payload.clone()],
        ),
        Event::Handoff {
            source,
            target,
            kind,
            fields,
        } => command_frame(
            RawPacketKind::Request,
            match kind {
                HandoffKind::Request => "HO",
                HandoffKind::Accept => "HA",
            },
            source.to_string(),
            target.to_string(),
            fields.clone(),
        ),
        Event::ClientData {
            source,
            target,
            kind,
            fields,
        } => encode_client_data(source, target, *kind, fields),
        Event::WeatherReport {
            source,
            destination,
            station: _,
            report,
        } => command_frame(
            RawPacketKind::Request,
            "AR",
            source.clone(),
            destination.to_string(),
            vec!["METAR".to_string(), report.clone()],
        ),
        Event::WindDelta { speed, direction } => command_frame(
            RawPacketKind::Client,
            "DL",
            "SERVER".to_string(),
            "*".to_string(),
            vec![speed.to_string(), direction.to_string()],
        ),
        Event::Disconnect { target, reason } => command_frame(
            RawPacketKind::Request,
            "!!",
            "SERVER".to_string(),
            target.to_string(),
            vec![reason.clone()],
        ),
        _ => Err(event_mismatch("routed")),
    }
}

fn encode_client_data(
    source: &Callsign,
    target: &Callsign,
    kind: ClientDataKind,
    fields: &[String],
) -> Result<WireFrame, ProtocolError> {
    let packet_kind = match kind {
        ClientDataKind::SquawkBox | ClientDataKind::ProController => RawPacketKind::Client,
        ClientDataKind::CommunicationRequest | ClientDataKind::CommunicationReply => {
            RawPacketKind::Request
        }
    };
    let command = match kind {
        ClientDataKind::SquawkBox => "SB",
        ClientDataKind::ProController => "PC",
        ClientDataKind::CommunicationRequest => "C?",
        ClientDataKind::CommunicationReply => "CI",
    };
    command_frame(
        packet_kind,
        command,
        source.to_string(),
        target.to_string(),
        fields.to_vec(),
    )
}
