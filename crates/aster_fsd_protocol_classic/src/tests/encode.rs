use aster_fsd_model::{
    Callsign, ClientDataKind, ClientPresence, ClientType, Command, Destination, Event, HandoffKind,
    QueryKind,
};
use aster_fsd_protocol::ProtocolBackend;

use crate::ClassicProtocol;

use super::support::{decode_context, encode_context, flight_plan};

#[test]
fn public_presence_never_contains_password() {
    let event = Event::ClientAdded {
        client: ClientPresence {
            callsign: Callsign::parse("ECP4143").unwrap(),
            client_type: ClientType::Pilot,
            network_id: "ECP1547".to_string(),
            real_name: "Test Pilot".to_string(),
            rating: 1,
            protocol_revision: 9,
            simulator_type: Some(2),
        },
    };
    let frame = ClassicProtocol
        .encode(&encode_context(), &event)
        .unwrap()
        .remove(0)
        .into_bytes();
    assert_eq!(frame.as_ref(), b"#APECP4143:SERVER:ECP1547::1:9:2");
    assert!(!std::str::from_utf8(&frame).unwrap().contains("secret"));
}

#[test]
fn typed_direct_and_ping_events_encode_to_c_exact_wire() {
    let ecp1 = Callsign::parse("ECP1").unwrap();
    let ecp2 = Callsign::parse("ECP2").unwrap();
    let cases = vec![
        (
            Event::Handoff {
                source: ecp1.clone(),
                target: ecp2.clone(),
                kind: HandoffKind::Request,
                fields: vec!["ECP3".to_string(), "123.450".to_string()],
            },
            b"$HOECP1:ECP2:ECP3:123.450".as_slice(),
        ),
        (
            Event::Handoff {
                source: ecp2.clone(),
                target: ecp1.clone(),
                kind: HandoffKind::Accept,
                fields: vec!["ECP3".to_string()],
            },
            b"$HAECP2:ECP1:ECP3".as_slice(),
        ),
        (
            Event::ClientData {
                source: ecp1.clone(),
                target: ecp2.clone(),
                kind: ClientDataKind::SquawkBox,
                fields: Vec::new(),
            },
            b"#SBECP1:ECP2".as_slice(),
        ),
        (
            Event::ClientData {
                source: ecp1.clone(),
                target: ecp2.clone(),
                kind: ClientDataKind::ProController,
                fields: vec!["VERSION".to_string()],
            },
            b"#PCECP1:ECP2:VERSION".as_slice(),
        ),
        (
            Event::ClientData {
                source: ecp1.clone(),
                target: ecp2.clone(),
                kind: ClientDataKind::CommunicationRequest,
                fields: Vec::new(),
            },
            b"$C?ECP1:ECP2".as_slice(),
        ),
        (
            Event::ClientData {
                source: ecp2.clone(),
                target: ecp1.clone(),
                kind: ClientDataKind::CommunicationReply,
                fields: vec!["123.450".to_string()],
            },
            b"$CIECP2:ECP1:123.450".as_slice(),
        ),
        (
            Event::Ping {
                source: ecp1.clone(),
                destination: Destination::All,
                payload: "nonce".to_string(),
            },
            b"$PIECP1:*:nonce".as_slice(),
        ),
        (
            Event::Pong {
                source: "server".to_string(),
                destination: Destination::Direct(ecp1),
                payload: "nonce".to_string(),
            },
            b"$POserver:ECP1:nonce".as_slice(),
        ),
    ];

    for (event, expected) in cases {
        let frames = ClassicProtocol.encode(&encode_context(), &event).unwrap();
        assert_eq!(frames.len(), 1, "{event:?}");
        assert_eq!(frames[0].as_bytes(), expected, "{event:?}");
    }
}

#[test]
fn query_response_and_flight_plan_destinations_encode_exactly() {
    let cases = [
        (
            Event::Query {
                source: "ECP1".to_string(),
                destination: Destination::Direct(Callsign::parse("ECP2").unwrap()),
                kind: QueryKind::AircraftConfiguration,
                arguments: vec!["CONFIG".to_string(), "FULL".to_string()],
            },
            b"$CQECP1:ECP2:ACC:CONFIG:FULL".as_slice(),
        ),
        (
            Event::Response {
                source: "ECP2".to_string(),
                destination: Destination::Direct(Callsign::parse("ECP1").unwrap()),
                kind: QueryKind::Capabilities,
                arguments: vec!["VERSION=1".to_string()],
            },
            b"$CRECP2:ECP1:CAPS:VERSION=1".as_slice(),
        ),
        (
            Event::FlightPlan {
                plan: flight_plan(),
                destination: Destination::Direct(Callsign::parse("ECP2").unwrap()),
            },
            b"$FPECP1:ECP2:I:B738:450:ZSPD:1200:1205:FL350:ZBAA:2:0:4:0:ZSNJ:RMK:DCT PIKAS DCT"
                .as_slice(),
        ),
    ];
    for (event, expected) in cases {
        let frames = ClassicProtocol.encode(&encode_context(), &event).unwrap();
        assert_eq!(frames[0].as_bytes(), expected, "{event:?}");
    }
}

#[test]
fn transponder_accepts_padding_and_uses_c_integer_wire_output() {
    for (input, expected) in [("0", "0"), ("0000", "0"), ("0700", "700")] {
        let frame = format!("@NECP1:{input}:1:31.23000:121.47000:5000:200:0:0");
        let command = ClassicProtocol
            .decode(&decode_context(), frame.as_bytes())
            .unwrap();
        let Command::Position(position) = command else {
            panic!("expected position command");
        };
        assert!(position.validate().is_ok());

        let frame = ClassicProtocol
            .encode(&encode_context(), &Event::Position { position })
            .unwrap()
            .remove(0)
            .into_bytes();
        assert_eq!(
            std::str::from_utf8(&frame).unwrap(),
            format!("@N:ECP1:{expected}:1:31.23000:121.47000:5000:200:0:0")
        );
    }
}
