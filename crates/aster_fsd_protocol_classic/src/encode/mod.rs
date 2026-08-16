mod presence;
mod routed;
mod state;
mod weather;

use aster_fsd_codec::{CLASSIC_MAX_FRAME_BYTES, RawPacket, RawPacketKind};
use aster_fsd_model::{Destination, Event};
use aster_fsd_protocol::{EncodeContext, ProtocolError, ProtocolErrorKind, WireFrame};

pub(super) fn encode(
    context: &EncodeContext,
    event: &Event,
) -> Result<Vec<WireFrame>, ProtocolError> {
    if let Event::WeatherProfile {
        destination,
        station: _,
        profile,
    } = event
    {
        return weather::encode_profile(destination, profile);
    }
    let frame = match event {
        Event::Error { .. } | Event::Welcome { .. } => presence::encode_status(event),
        Event::ClientAdded { .. } | Event::ClientRemoved { .. } => presence::encode_presence(event),
        Event::Position { position } => state::encode_position(position),
        Event::FlightPlan { plan, destination } => state::encode_flight_plan(plan, destination),
        Event::Query { .. } | Event::Response { .. } => routed::encode_query(event),
        Event::Text { .. }
        | Event::Ping { .. }
        | Event::Pong { .. }
        | Event::Handoff { .. }
        | Event::ClientData { .. }
        | Event::WeatherReport { .. }
        | Event::WindDelta { .. }
        | Event::Disconnect { .. } => routed::encode(context, event),
        Event::WeatherProfile { .. } => {
            return Err(event_mismatch("weather profile"));
        }
    }?;
    Ok(vec![frame])
}

fn destination_text(destination: &Destination) -> String {
    match destination {
        Destination::Server => "SERVER".to_string(),
        Destination::Direct(callsign) => callsign.to_string(),
        Destination::All => "*".to_string(),
        Destination::Atc => "*A".to_string(),
        Destination::Pilots => "*P".to_string(),
        Destination::Range(value) => value.clone(),
    }
}

fn frame(packet: &RawPacket) -> Result<WireFrame, ProtocolError> {
    packet
        .encode(CLASSIC_MAX_FRAME_BYTES)
        .map_err(|error| ProtocolError::new(ProtocolErrorKind::Encoding, error.to_string()))
        .and_then(WireFrame::new)
}

fn command_frame(
    kind: RawPacketKind,
    command: &str,
    source: String,
    destination: String,
    fields: Vec<String>,
) -> Result<WireFrame, ProtocolError> {
    frame(&RawPacket {
        kind,
        command: command.to_string(),
        source,
        destination,
        fields,
    })
}

fn event_mismatch(category: &'static str) -> ProtocolError {
    ProtocolError::new(
        ProtocolErrorKind::Encoding,
        format!("classic {category} encoder received an unrelated event"),
    )
}
