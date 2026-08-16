use aster_fsd_codec::CLASSIC_MAX_FRAME_BYTES;
use aster_fsd_model::{Callsign, Command, ConnectionId, Destination, ErrorCode, Event};
use aster_fsd_protocol::{HandshakeContext, ProtocolBackend, ProtocolErrorKind};

use crate::ClassicProtocol;

use super::support::{decode_context, encode_context};

#[test]
fn classic_is_client_speaks_first() {
    let context = HandshakeContext {
        connection_id: ConnectionId(1),
        peer: "127.0.0.1:1234".parse().unwrap(),
        server_name: "AsterFSD".to_string(),
        server_version: "0.2.0".to_string(),
        challenge: "unused".to_string(),
    };
    assert!(ClassicProtocol.initial_frames(&context).unwrap().is_empty());
}

#[test]
fn error_wire_matches_classic_shape() {
    for (code, expected_description) in [
        (ErrorCode::NoError, "No error"),
        (ErrorCode::CallsignInUse, "Callsign in use"),
        (ErrorCode::InvalidCallsign, "Invalid callsign"),
        (ErrorCode::AlreadyRegistered, "Already registerd"),
        (ErrorCode::Syntax, "Syntax error"),
        (ErrorCode::InvalidSource, "Invalid source callsign"),
        (ErrorCode::InvalidCredentials, "Invalid CID/password"),
        (ErrorCode::NoSuchCallsign, "No such callsign"),
        (ErrorCode::NoFlightPlan, "No flightplan"),
        (ErrorCode::NoWeather, "No such weather profile"),
        (
            ErrorCode::InvalidProtocolRevision,
            "Invalid protocol revision",
        ),
        (ErrorCode::RequestedLevelTooHigh, "Requested level too high"),
        (ErrorCode::ServerFull, "Too many clients connected"),
        (ErrorCode::Suspended, "CID/PID was suspended"),
    ] {
        assert_eq!(code.description(), expected_description);
        let event = Event::Error {
            callsign: Some(Callsign::parse("ECP4143").unwrap()),
            code,
            environment: if code == ErrorCode::InvalidCredentials {
                "ECP1547".to_string()
            } else {
                String::new()
            },
            description: expected_description.to_string(),
        };
        let frame = ClassicProtocol
            .encode(&encode_context(), &event)
            .unwrap()
            .remove(0)
            .into_bytes();
        assert_eq!(
            std::str::from_utf8(&frame).unwrap(),
            format!(
                "$ERserver:ECP4143:{:03}:{}:{}",
                code as u16,
                if code == ErrorCode::InvalidCredentials {
                    "ECP1547"
                } else {
                    ""
                },
                expected_description
            )
        );
    }
}

#[test]
fn login_field_minimums_and_revision_errors_match_classic_contract() {
    for frame in [
        b"#AAATC1:SERVER:Name:CID:secret:5".as_slice(),
        b"#APPILOT1:SERVER:CID:secret:1:9:2",
        b"#APX:SERVER:CID:secret",
    ] {
        let error = ClassicProtocol
            .decode(&decode_context(), frame)
            .unwrap_err();
        assert_eq!(error.kind, ProtocolErrorKind::Syntax, "{frame:?}");
        assert_eq!(error.error_code, None, "{frame:?}");
    }

    for frame in [
        b"#AAATC1:SERVER:Name:CID:secret:5:not-a-number".as_slice(),
        b"#APPILOT1:SERVER:CID:secret:1:65536:2:Name",
    ] {
        let error = ClassicProtocol
            .decode(&decode_context(), frame)
            .unwrap_err();
        assert_eq!(error.kind, ProtocolErrorKind::InvalidField, "{frame:?}");
        assert_eq!(
            error.error_code,
            Some(ErrorCode::InvalidProtocolRevision),
            "{frame:?}"
        );
    }
}

#[test]
fn malformed_and_unsupported_packets_have_distinct_error_categories() {
    let malformed = ClassicProtocol
        .decode(&decode_context(), b"#APECP1:SERVER:CID:password")
        .unwrap_err();
    assert_eq!(malformed.kind, ProtocolErrorKind::Syntax);

    let unsupported = ClassicProtocol
        .decode(&decode_context(), b"$ZZECP1:SERVER:payload")
        .unwrap_err();
    assert_eq!(unsupported.kind, ProtocolErrorKind::Unsupported);

    let invalid_position = ClassicProtocol
        .decode(&decode_context(), b"@NECP1:1200:1:91:0:0:0:0:0")
        .unwrap();
    let Command::Position(position) = invalid_position else {
        panic!("expected position command");
    };
    assert!(position.validate().is_err());
}

#[test]
fn c_payload_minimums_cover_text_acars_query_response_and_kill() {
    for frame in [
        b"#TMECP1:ECP2".as_slice(),
        b"$AXECP1:SERVER",
        b"$CRECP1:ECP2:CAPS",
        b"$!!ECP1:ECP2",
    ] {
        let error = ClassicProtocol
            .decode(&decode_context(), frame)
            .unwrap_err();
        assert_eq!(error.kind, ProtocolErrorKind::Syntax, "{frame:?}");
    }

    assert!(matches!(
        ClassicProtocol
            .decode(&decode_context(), b"$!!ECP1:ECP2:network abuse")
            .unwrap(),
        Command::Kill {
            source,
            target,
            reason,
        } if source.as_str() == "ECP1"
            && target.as_str() == "ECP2"
            && reason == "network abuse"
    ));
}

#[test]
fn classic_encoder_rejects_frames_over_draft_nine_limit() {
    let event = Event::Text {
        source: Callsign::parse("ECP1").unwrap(),
        destination: Destination::All,
        message: "x".repeat(CLASSIC_MAX_FRAME_BYTES),
    };
    let error = ClassicProtocol
        .encode(&encode_context(), &event)
        .unwrap_err();
    assert_eq!(error.kind, ProtocolErrorKind::Encoding);
}
