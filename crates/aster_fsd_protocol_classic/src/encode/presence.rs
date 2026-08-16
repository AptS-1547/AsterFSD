use aster_fsd_codec::RawPacketKind;
use aster_fsd_model::{CLASSIC_PROTOCOL_REVISION, ClientType, Event};
use aster_fsd_protocol::{ProtocolError, WireFrame};

use super::{command_frame, event_mismatch};

pub(super) fn encode_status(event: &Event) -> Result<WireFrame, ProtocolError> {
    match event {
        Event::Error {
            callsign,
            code,
            environment,
            description,
        } => command_frame(
            RawPacketKind::Request,
            "ER",
            "server".to_string(),
            callsign
                .as_ref()
                .map_or_else(|| "unknown".to_string(), ToString::to_string),
            vec![
                format!("{:03}", *code as u16),
                environment.clone(),
                description.clone(),
            ],
        ),
        Event::Welcome { callsign, message } => command_frame(
            RawPacketKind::Client,
            "TM",
            "server".to_string(),
            callsign.to_string(),
            vec![message.clone()],
        ),
        _ => Err(event_mismatch("status")),
    }
}

pub(super) fn encode_presence(event: &Event) -> Result<WireFrame, ProtocolError> {
    match event {
        Event::ClientAdded { client } => {
            let (command, fields) = match client.client_type {
                ClientType::Atc => (
                    "AA",
                    vec![
                        client.real_name.clone(),
                        client.network_id.clone(),
                        String::new(),
                        client.rating.to_string(),
                        CLASSIC_PROTOCOL_REVISION.to_string(),
                    ],
                ),
                ClientType::Pilot | ClientType::Observer => (
                    "AP",
                    vec![
                        client.network_id.clone(),
                        String::new(),
                        client.rating.to_string(),
                        CLASSIC_PROTOCOL_REVISION.to_string(),
                        client.simulator_type.unwrap_or_default().to_string(),
                    ],
                ),
            };
            command_frame(
                RawPacketKind::Client,
                command,
                client.callsign.to_string(),
                "SERVER".to_string(),
                fields,
            )
        }
        Event::ClientRemoved {
            callsign,
            client_type,
            network_id,
        } => command_frame(
            RawPacketKind::Client,
            if *client_type == ClientType::Atc {
                "DA"
            } else {
                "DP"
            },
            callsign.to_string(),
            network_id.clone(),
            Vec::new(),
        ),
        _ => Err(event_mismatch("presence")),
    }
}
