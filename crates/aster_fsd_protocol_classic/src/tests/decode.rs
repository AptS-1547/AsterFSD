use aster_fsd_model::{ClientDataKind, Command, ErrorCode, HandoffKind, QueryKind};
use aster_fsd_protocol::{ProtocolBackend, ProtocolErrorKind};

use crate::ClassicProtocol;

use super::support::decode_context;

#[test]
fn pilot_login_is_typed_and_keeps_password_only_in_command() {
    let command = ClassicProtocol
        .decode(
            &decode_context(),
            b"#APECP4143:SERVER:ECP1547:secret:1:9:2:Test Pilot",
        )
        .unwrap();
    let Command::Login(login) = command else {
        panic!("expected login");
    };
    assert_eq!(login.protocol_revision, 9);
    assert_eq!(login.password, "secret");
}

#[test]
fn invalid_login_callsign_maps_to_classic_error_two() {
    let error = ClassicProtocol
        .decode(
            &decode_context(),
            b"#APBAD CALL:SERVER:ECP1547:secret:1:9:2:Test Pilot",
        )
        .unwrap_err();
    assert_eq!(error.error_code, Some(ErrorCode::InvalidCallsign));
}

#[test]
fn c_direct_only_commands_reject_multicast_destinations() {
    for frame in [
        b"$HOECP1:*:payload".as_slice(),
        b"$HAECP1:*A:payload",
        b"#SBECP1:*P:payload",
        b"#PCECP1:@94836:payload",
        b"$C?ECP1:*:payload",
        b"$CIECP1:*:payload",
        b"$CRECP1:*:RN:payload",
    ] {
        let error = ClassicProtocol
            .decode(&decode_context(), frame)
            .unwrap_err();
        assert_eq!(error.kind, ProtocolErrorKind::Syntax, "{frame:?}");
    }
}

#[test]
fn c_command_directions_decode_to_typed_direct_delivery() {
    for (frame, expected_kind) in [
        (b"$HOECP1:ECP2:payload".as_slice(), HandoffKind::Request),
        (b"$HAECP1:ECP2:payload", HandoffKind::Accept),
    ] {
        let command = ClassicProtocol.decode(&decode_context(), frame).unwrap();
        assert!(matches!(
            command,
            Command::Handoff {
                target,
                kind,
                fields,
                ..
            } if target.as_str() == "ECP2"
                && kind == expected_kind
                && fields == ["payload"]
        ));
    }

    for (frame, expected_kind) in [
        (
            b"#SBECP1:ECP2:payload".as_slice(),
            ClientDataKind::SquawkBox,
        ),
        (b"#PCECP1:ECP2:payload", ClientDataKind::ProController),
        (
            b"$C?ECP1:ECP2:payload",
            ClientDataKind::CommunicationRequest,
        ),
        (b"$CIECP1:ECP2:payload", ClientDataKind::CommunicationReply),
    ] {
        let command = ClassicProtocol.decode(&decode_context(), frame).unwrap();
        assert!(matches!(
            command,
            Command::ClientData {
                target,
                kind,
                fields,
                ..
            } if target.as_str() == "ECP2"
                && kind == expected_kind
                && fields == ["payload"]
        ));
    }

    for frame in [b"$HOECP1:ECP2".as_slice(), b"$HAECP1:ECP2", b"$CIECP1:ECP2"] {
        let error = ClassicProtocol
            .decode(&decode_context(), frame)
            .unwrap_err();
        assert_eq!(error.kind, ProtocolErrorKind::Syntax, "{frame:?}");
    }
}

#[test]
fn query_packets_decode_to_typed_kinds() {
    for (frame, expected_kind, expected_arguments) in [
        (
            b"$CQECP1:ECP2:CAPS".as_slice(),
            QueryKind::Capabilities,
            Vec::<String>::new(),
        ),
        (
            b"$CQECP1:ECP2:ACC:CONFIG:FULL".as_slice(),
            QueryKind::AircraftConfiguration,
            vec!["CONFIG".to_string(), "FULL".to_string()],
        ),
        (b"$CQECP1:ECP2:ATIS".as_slice(), QueryKind::Atis, Vec::new()),
        (
            b"$CQECP1:ECP2:INF".as_slice(),
            QueryKind::SystemInfo,
            Vec::new(),
        ),
    ] {
        let command = ClassicProtocol.decode(&decode_context(), frame).unwrap();
        assert!(matches!(
            command,
            Command::Query {
                source,
                destination: aster_fsd_model::Destination::Direct(target),
                kind,
                arguments,
            } if source.as_str() == "ECP1"
                && target.as_str() == "ECP2"
                && kind == expected_kind
                && arguments == expected_arguments
        ));
    }

    let response = ClassicProtocol
        .decode(&decode_context(), b"$CRECP2:ECP1:CAPS:VERSION=1:ATCINFO=1")
        .unwrap();
    assert!(matches!(
        response,
        Command::Response {
            source,
            destination: aster_fsd_model::Destination::Direct(target),
            kind: QueryKind::Capabilities,
            arguments,
        } if source.as_str() == "ECP2"
            && target.as_str() == "ECP1"
            && arguments == ["VERSION=1", "ATCINFO=1"]
    ));

    let missing_response_payload = ClassicProtocol
        .decode(&decode_context(), b"$CRECP2:ECP1:CAPS")
        .unwrap_err();
    assert_eq!(missing_response_payload.kind, ProtocolErrorKind::Syntax);
}
