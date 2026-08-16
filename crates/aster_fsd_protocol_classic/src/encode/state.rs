use aster_fsd_codec::{RawPacket, RawPacketKind};
use aster_fsd_model::{Destination, FlightPlan, Position};
use aster_fsd_protocol::{ProtocolError, WireFrame};

use super::{command_frame, destination_text, frame};

pub(super) fn encode_position(position: &Position) -> Result<WireFrame, ProtocolError> {
    let packet = match position {
        Position::Pilot(position) => RawPacket {
            kind: RawPacketKind::PilotPosition,
            command: position.mode.to_string(),
            source: position.callsign.to_string(),
            destination: String::new(),
            fields: vec![
                position.squawk.clone(),
                position.rating.to_string(),
                format!("{:.5}", position.latitude),
                format!("{:.5}", position.longitude),
                position.altitude.to_string(),
                position.groundspeed.to_string(),
                position.pbh.to_string(),
                position.flags.to_string(),
            ],
        },
        Position::Atc(position) => RawPacket {
            kind: RawPacketKind::AtcPosition,
            command: String::new(),
            source: position.callsign.to_string(),
            destination: String::new(),
            fields: vec![
                position.frequency.to_string(),
                position.facility_type.to_string(),
                position.visual_range.to_string(),
                position.rating.to_string(),
                format!("{:.5}", position.latitude),
                format!("{:.5}", position.longitude),
                position.altitude.to_string(),
            ],
        },
    };
    frame(&packet)
}

pub(super) fn encode_flight_plan(
    plan: &FlightPlan,
    destination: &Destination,
) -> Result<WireFrame, ProtocolError> {
    command_frame(
        RawPacketKind::Request,
        "FP",
        plan.callsign.to_string(),
        destination_text(destination),
        vec![
            plan.flight_rules.to_string(),
            plan.aircraft.clone(),
            plan.cruise_speed.to_string(),
            plan.departure.clone(),
            plan.estimated_departure.to_string(),
            plan.actual_departure.to_string(),
            plan.cruise_altitude.clone(),
            plan.destination.clone(),
            plan.hours_enroute.to_string(),
            plan.minutes_enroute.to_string(),
            plan.hours_fuel.to_string(),
            plan.minutes_fuel.to_string(),
            plan.alternate.clone(),
            plan.remarks.clone(),
            plan.route.clone(),
        ],
    )
}
