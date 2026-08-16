use crate::Callsign;
use serde::{Deserialize, Serialize};

/// Filed flight plan stored as authoritative shared network state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlightPlan {
    pub callsign: Callsign,
    pub flight_rules: char,
    pub aircraft: String,
    pub cruise_speed: i32,
    pub departure: String,
    pub estimated_departure: i32,
    pub actual_departure: i32,
    pub cruise_altitude: String,
    pub destination: String,
    pub hours_enroute: i32,
    pub minutes_enroute: i32,
    pub hours_fuel: i32,
    pub minutes_fuel: i32,
    pub alternate: String,
    pub remarks: String,
    pub route: String,
}
