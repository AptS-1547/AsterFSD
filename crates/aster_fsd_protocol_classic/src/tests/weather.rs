use aster_fsd_model::{Callsign, Command, Event};
use aster_fsd_protocol::{ProtocolBackend, ProtocolErrorKind, WireFrame};

use crate::ClassicProtocol;

use super::support::{decode_context, encode_context, weather_profile};

#[test]
fn c_weather_requests_select_parsed_and_raw_forms() {
    let parsed = ClassicProtocol
        .decode(&decode_context(), b"#WXECP1:SERVER:kjfk:ignored")
        .unwrap();
    assert!(matches!(
        parsed,
        Command::WeatherRequest {
            source,
            station,
            parsed: true,
        } if source.as_str() == "ECP1" && station == "KJFK"
    ));

    let raw = ClassicProtocol
        .decode(&decode_context(), b"$AXECP1:SERVER:metar:kjfk")
        .unwrap();
    assert!(matches!(
        raw,
        Command::WeatherRequest {
            source,
            station,
            parsed: false,
        } if source.as_str() == "ECP1" && station == "KJFK"
    ));

    for frame in [
        b"$AXECP1:SERVER:METAR".as_slice(),
        b"$AXECP1:SERVER:TAF:KJFK",
    ] {
        assert!(matches!(
            ClassicProtocol.decode(&decode_context(), frame).unwrap(),
            Command::Noop { source } if source.as_str() == "ECP1"
        ));
    }
}

#[test]
fn c_parsed_weather_profile_encodes_exact_td_wd_cd_frames() {
    let event = Event::WeatherProfile {
        destination: Callsign::parse("ECP1").unwrap(),
        station: "KJFK".to_string(),
        profile: weather_profile(),
    };
    let frames = ClassicProtocol.encode(&encode_context(), &event).unwrap();
    let actual: Vec<_> = frames.iter().map(WireFrame::as_bytes).collect();
    assert_eq!(
        actual,
        vec![
            b"#TDserver:ECP1:100:15:10000:-5:18000:-21:35000:-51:2992".as_slice(),
            b"#WDserver:ECP1:2500:0:180:12:0:1:10400:2500:190:22:1:2:22600:10400:210:35:0:3:90000:22700:240:55:1:4".as_slice(),
            b"#CDserver:ECP1:5000:3000:4:0:1:12000:10000:2:1:0:35000:20000:1:2:3:12.50".as_slice(),
        ]
    );

    let mut invalid = weather_profile();
    invalid.visibility = f64::NAN;
    let error = ClassicProtocol
        .encode(
            &encode_context(),
            &Event::WeatherProfile {
                destination: Callsign::parse("ECP1").unwrap(),
                station: "KJFK".to_string(),
                profile: invalid,
            },
        )
        .unwrap_err();
    assert_eq!(error.kind, ProtocolErrorKind::Encoding);
}
