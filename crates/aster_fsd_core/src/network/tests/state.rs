use super::{atc_login, atc_position, network, pilot_login, pilot_position};
use crate::{Delivery, Network};
use aster_fsd_model::{
    AtcPosition, Callsign, Command, ConnectionId, Destination, ErrorCode, Event, PilotPosition,
    Position, ProtocolDialect,
};

#[tokio::test]
async fn source_spoof_is_rejected_without_broadcast() {
    let network = network();
    network
        .register(
            ConnectionId(1),
            "127.0.0.1:1000".parse().unwrap(),
            ProtocolDialect::Classic,
        )
        .await
        .unwrap();
    network.execute(ConnectionId(1), pilot_login("ECP1")).await;
    let effects = network
        .execute(
            ConnectionId(1),
            Command::Text {
                source: Callsign::parse("ECP2").unwrap(),
                destination: Destination::All,
                message: "spoof".to_string(),
            },
        )
        .await;
    assert!(matches!(
        &effects.deliveries[0],
        Delivery { recipients, event }
            if recipients == &vec![ConnectionId(1)]
                && matches!(event, Event::Error {
                    code: ErrorCode::InvalidSource,
                    ..
                })
    ));
}

#[tokio::test]
async fn classic_integer_transponder_codes_are_normal_position_updates() {
    let network = network();
    for id in [ConnectionId(1), ConnectionId(2)] {
        network
            .register(
                id,
                "127.0.0.1:1000".parse().unwrap(),
                ProtocolDialect::Classic,
            )
            .await
            .unwrap();
    }
    network.execute(ConnectionId(1), pilot_login("ECP1")).await;
    network.execute(ConnectionId(2), pilot_login("ECP2")).await;
    for (id, callsign, squawk) in [
        (ConnectionId(2), "ECP2", "0"),
        (ConnectionId(1), "ECP1", "7500"),
    ] {
        let effects = network
            .execute(
                id,
                Command::Position(Position::Pilot(PilotPosition {
                    callsign: Callsign::parse(callsign).unwrap(),
                    mode: 'S',
                    squawk: squawk.to_string(),
                    rating: 1,
                    latitude: 40.0,
                    longitude: -73.0,
                    altitude: 5_000,
                    groundspeed: 200,
                    pbh: 0,
                    flags: 0,
                })),
            )
            .await;
        assert!(effects.close.is_none());
        assert!(
            !effects
                .deliveries
                .iter()
                .any(|delivery| matches!(delivery.event, Event::Error { .. }))
        );
        if id == ConnectionId(1) {
            assert!(effects.deliveries.iter().any(|delivery| matches!(
                &delivery.event,
                Event::Position {
                    position: Position::Pilot(position),
                } if position.squawk == squawk
            )));
        }
    }
}

#[tokio::test]
async fn invalid_pilot_rating_gap_does_not_claim_callsign_and_repeated_login_is_rejected() {
    let network = network();
    for id in [ConnectionId(1), ConnectionId(2)] {
        network
            .register(
                id,
                "127.0.0.1:1000".parse().unwrap(),
                ProtocolDialect::Classic,
            )
            .await
            .unwrap();
    }
    let Command::Login(mut invalid) = pilot_login("ECP1") else {
        panic!("expected login command");
    };
    invalid.requested_rating = 2;
    let effects = network
        .execute(ConnectionId(1), Command::Login(invalid))
        .await;
    assert!(effects.deliveries.iter().any(|delivery| matches!(
        delivery.event,
        Event::Error {
            code: ErrorCode::RequestedLevelTooHigh,
            ..
        }
    )));

    network.execute(ConnectionId(2), pilot_login("ECP1")).await;
    let repeated = network.execute(ConnectionId(2), pilot_login("ECP1")).await;
    assert!(repeated.deliveries.iter().any(|delivery| matches!(
        delivery.event,
        Event::Error {
            code: ErrorCode::AlreadyRegistered,
            ..
        }
    )));
}

#[tokio::test]
async fn active_position_type_mismatch_is_rejected() {
    let network = network();
    network
        .register(
            ConnectionId(1),
            "127.0.0.1:1000".parse().unwrap(),
            ProtocolDialect::Classic,
        )
        .await
        .unwrap();
    network.execute(ConnectionId(1), pilot_login("ECP1")).await;
    let effects = network
        .execute(
            ConnectionId(1),
            Command::Position(Position::Atc(AtcPosition {
                callsign: Callsign::parse("ECP1").unwrap(),
                frequency: 199_998,
                facility_type: 5,
                visual_range: 100,
                rating: 1,
                latitude: 0.0,
                longitude: 0.0,
                altitude: 0,
            })),
        )
        .await;
    assert!(effects.deliveries.iter().any(|delivery| matches!(
        delivery.event,
        Event::Error {
            code: ErrorCode::Syntax,
            ..
        }
    )));
}

#[tokio::test]
async fn invalid_or_spoofed_position_never_overwrites_authoritative_state() {
    let network = network();
    network
        .register(
            ConnectionId(1),
            "127.0.0.1:1000".parse().unwrap(),
            ProtocolDialect::Classic,
        )
        .await
        .unwrap();
    network.execute(ConnectionId(1), pilot_login("ECP1")).await;
    network
        .execute(
            ConnectionId(1),
            pilot_position("ECP1", 31.23, 121.47, 5_000),
        )
        .await;

    let mut invalid = pilot_position("ECP1", 91.0, 121.47, 10_000);
    let invalid_effects = network.execute(ConnectionId(1), invalid.clone()).await;
    assert!(matches!(
        invalid_effects.deliveries.as_slice(),
        [Delivery {
            event: Event::Error {
                code: ErrorCode::Syntax,
                ..
            },
            ..
        }]
    ));

    if let Command::Position(Position::Pilot(position)) = &mut invalid {
        position.callsign = Callsign::parse("ECP2").unwrap();
        position.latitude = 32.0;
    }
    let spoof_effects = network.execute(ConnectionId(1), invalid).await;
    assert!(matches!(
        spoof_effects.deliveries.as_slice(),
        [Delivery {
            event: Event::Error {
                code: ErrorCode::InvalidSource,
                ..
            },
            ..
        }]
    ));

    let state = network.state.read().await;
    let Position::Pilot(position) = state
        .sessions
        .get(&ConnectionId(1))
        .unwrap()
        .position
        .as_ref()
        .unwrap()
    else {
        panic!("expected authoritative pilot position");
    };
    assert!((position.latitude - 31.23).abs() < f64::EPSILON);
    assert!((position.longitude - 121.47).abs() < f64::EPSILON);
    assert_eq!(position.altitude, 5_000);
}

#[tokio::test]
async fn c_position_ranges_cover_pilot_atc_and_missing_position_boundaries() {
    let network = network();
    for id in [ConnectionId(1), ConnectionId(2), ConnectionId(3)] {
        network
            .register(
                id,
                "127.0.0.1:1000".parse().unwrap(),
                ProtocolDialect::Classic,
            )
            .await
            .unwrap();
    }
    network
        .execute(ConnectionId(1), pilot_login("PILOT1"))
        .await;
    network
        .execute(ConnectionId(2), pilot_login("PILOT2"))
        .await;
    network.execute(ConnectionId(3), atc_login("ATC1", 5)).await;

    network
        .execute(ConnectionId(1), pilot_position("PILOT1", 0.0, 0.0, 0))
        .await;
    network
        .execute(ConnectionId(2), pilot_position("PILOT2", 1.0, 0.0, 1_600))
        .await;

    {
        let state = network.state.read().await;
        assert!(
            (Network::session_range(state.sessions.get(&ConnectionId(1)).unwrap()) - 10.0).abs()
                < f64::EPSILON
        );
        assert!(
            (Network::session_range(state.sessions.get(&ConnectionId(2)).unwrap()) - 66.0).abs()
                < f64::EPSILON
        );
        assert!(Network::within_position_range(
            &state,
            ConnectionId(1),
            ConnectionId(2)
        ));
        assert!(!Network::within_position_range(
            &state,
            ConnectionId(1),
            ConnectionId(3)
        ));
    }

    network
        .execute(ConnectionId(3), atc_position("ATC1", 1.0, 0.0, 5, 50))
        .await;
    {
        let state = network.state.read().await;
        assert!(!Network::within_position_range(
            &state,
            ConnectionId(1),
            ConnectionId(3)
        ));
    }

    network
        .execute(ConnectionId(3), atc_position("ATC1", 1.0, 0.0, 5, 61))
        .await;
    let state = network.state.read().await;
    assert!(Network::within_position_range(
        &state,
        ConnectionId(1),
        ConnectionId(3)
    ));
    assert!(Network::within_position_range(
        &state,
        ConnectionId(3),
        ConnectionId(1)
    ));
}
