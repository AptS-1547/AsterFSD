use aster_fsd_model::{
    Callsign, CloudLayer, ConnectionId, FlightPlan, SessionPhase, TemperatureLayer, WeatherProfile,
    WindLayer,
};
use aster_fsd_protocol::{DecodeContext, EncodeContext};

pub(super) fn decode_context() -> DecodeContext {
    DecodeContext {
        connection_id: ConnectionId(1),
        phase: SessionPhase::Connected,
        callsign: None,
        challenge: String::new(),
    }
}

pub(super) fn encode_context() -> EncodeContext {
    EncodeContext {
        connection_id: ConnectionId(2),
        recipient: None,
        server_name: "AsterFSD".to_string(),
    }
}

pub(super) fn weather_profile() -> WeatherProfile {
    WeatherProfile {
        temperatures: [
            TemperatureLayer {
                ceiling: 100,
                temperature: 15,
            },
            TemperatureLayer {
                ceiling: 10_000,
                temperature: -5,
            },
            TemperatureLayer {
                ceiling: 18_000,
                temperature: -21,
            },
            TemperatureLayer {
                ceiling: 35_000,
                temperature: -51,
            },
        ],
        winds: [
            WindLayer {
                ceiling: 2_500,
                floor: 0,
                direction: 180,
                speed: 12,
                gusting: 0,
                turbulence: 1,
            },
            WindLayer {
                ceiling: 10_400,
                floor: 2_500,
                direction: 190,
                speed: 22,
                gusting: 1,
                turbulence: 2,
            },
            WindLayer {
                ceiling: 22_600,
                floor: 10_400,
                direction: 210,
                speed: 35,
                gusting: 0,
                turbulence: 3,
            },
            WindLayer {
                ceiling: 90_000,
                floor: 22_700,
                direction: 240,
                speed: 55,
                gusting: 1,
                turbulence: 4,
            },
        ],
        clouds: [
            CloudLayer {
                ceiling: 5_000,
                floor: 3_000,
                coverage: 4,
                icing: 0,
                turbulence: 1,
            },
            CloudLayer {
                ceiling: 12_000,
                floor: 10_000,
                coverage: 2,
                icing: 1,
                turbulence: 0,
            },
        ],
        thunderstorm: CloudLayer {
            ceiling: 35_000,
            floor: 20_000,
            coverage: 1,
            icing: 2,
            turbulence: 3,
        },
        barometer: 2_992,
        visibility: 12.5,
    }
}

pub(super) fn flight_plan() -> FlightPlan {
    FlightPlan {
        callsign: Callsign::parse("ECP1").unwrap(),
        flight_rules: 'I',
        aircraft: "B738".to_string(),
        cruise_speed: 450,
        departure: "ZSPD".to_string(),
        estimated_departure: 1200,
        actual_departure: 1205,
        cruise_altitude: "FL350".to_string(),
        destination: "ZBAA".to_string(),
        hours_enroute: 2,
        minutes_enroute: 0,
        hours_fuel: 4,
        minutes_fuel: 0,
        alternate: "ZSNJ".to_string(),
        remarks: "RMK".to_string(),
        route: "DCT PIKAS DCT".to_string(),
    }
}
