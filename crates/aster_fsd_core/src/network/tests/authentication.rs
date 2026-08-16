use super::{
    AuthenticationFailure, FailingAuthenticator, network, pilot_login, vatsim_atc_login,
    vatsim_identification, vatsim_pilot_login,
};
use crate::{CoreConfig, Delivery, Network};
use aster_fsd_auth::AllowAllAuthenticator;
use aster_fsd_model::{
    Command, ConnectionId, Destination, ErrorCode, Event, ProtocolDialect, QueryKind, SessionPhase,
};
use std::sync::Arc;

#[tokio::test]
async fn login_uses_the_configured_product_message() {
    let network = Network::new(
        CoreConfig {
            product_message: "Welcome to AsterFSD test".to_string(),
            ..CoreConfig::default()
        },
        Arc::new(AllowAllAuthenticator),
    );
    network
        .register(
            ConnectionId(1),
            "127.0.0.1:1000".parse().unwrap(),
            ProtocolDialect::Classic,
        )
        .await
        .unwrap();

    let effects = network.execute(ConnectionId(1), pilot_login("ECP1")).await;
    assert!(effects.deliveries.iter().any(|delivery| matches!(
        &delivery.event,
        Event::Welcome { message, .. } if message == "Welcome to AsterFSD test"
    )));
}

#[tokio::test]
async fn duplicate_callsign_never_replaces_the_existing_owner() {
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
    let effects = network.execute(ConnectionId(2), pilot_login("ecp1")).await;
    assert!(effects.deliveries.iter().any(|effect| matches!(
        effect,
        Delivery { event, .. }
            if matches!(event, Event::Error {
                code: ErrorCode::CallsignInUse,
                ..
            })
    )));
    assert_eq!(
        network.snapshot(ConnectionId(1)).await.unwrap().phase,
        SessionPhase::Active
    );
}

#[tokio::test]
async fn concurrent_duplicate_login_has_exactly_one_authoritative_owner() {
    let network = Arc::new(network());
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

    let first_network = network.clone();
    let second_network = network.clone();
    let (first, second) = tokio::join!(
        first_network.execute(ConnectionId(1), pilot_login("ECP1")),
        second_network.execute(ConnectionId(2), pilot_login("ecp1")),
    );

    let successful_logins = [first.clone(), second.clone()]
        .iter()
        .filter(|effects| {
            effects
                .deliveries
                .iter()
                .any(|delivery| matches!(delivery.event, Event::Welcome { .. }))
        })
        .count();
    let rejected_logins = [first, second]
        .iter()
        .filter(|effects| {
            effects.close.is_some()
                && effects.deliveries.iter().any(|delivery| {
                    matches!(
                        delivery.event,
                        Event::Error {
                            code: ErrorCode::CallsignInUse,
                            ..
                        }
                    )
                })
        })
        .count();
    assert_eq!(successful_logins, 1);
    assert_eq!(rejected_logins, 1);

    let state = network.state.read().await;
    let active_sessions = state
        .sessions
        .values()
        .filter(|session| session.phase == SessionPhase::Active)
        .count();
    assert_eq!(active_sessions, 1);
    assert_eq!(state.callsigns.len(), 1);
}

#[tokio::test]
async fn login_failures_are_closed_without_claiming_a_callsign() {
    for (failure, expected_code) in [
        (
            AuthenticationFailure::InvalidCredentials,
            ErrorCode::InvalidCredentials,
        ),
        (AuthenticationFailure::Suspended, ErrorCode::Suspended),
    ] {
        let network = Network::new(
            CoreConfig::default(),
            Arc::new(FailingAuthenticator(failure)),
        );
        network
            .register(
                ConnectionId(1),
                "127.0.0.1:1000".parse().unwrap(),
                ProtocolDialect::Classic,
            )
            .await
            .unwrap();

        let effects = network.execute(ConnectionId(1), pilot_login("ECP1")).await;
        assert!(effects.close.is_some());
        assert!(matches!(
            effects.deliveries.as_slice(),
            [Delivery {
                recipients,
                event: Event::Error { code, .. },
            }] if recipients == &[ConnectionId(1)] && *code == expected_code
        ));
        let state = network.state.read().await;
        assert!(state.callsigns.is_empty());
        assert_eq!(
            state.sessions.get(&ConnectionId(1)).unwrap().phase,
            SessionPhase::Connected
        );
    }
}

#[tokio::test]
async fn classic_revision_failure_has_empty_environment_and_closes() {
    let network = network();
    network
        .register(
            ConnectionId(1),
            "127.0.0.1:1000".parse().unwrap(),
            ProtocolDialect::Classic,
        )
        .await
        .unwrap();
    let Command::Login(mut login) = pilot_login("ECP1") else {
        panic!("expected login command");
    };
    login.protocol_revision = 8;

    let effects = network
        .execute(ConnectionId(1), Command::Login(login))
        .await;
    assert!(effects.close.is_some());
    assert!(matches!(
        effects.deliveries.as_slice(),
        [Delivery {
            event: Event::Error {
                code: ErrorCode::InvalidProtocolRevision,
                environment,
                ..
            },
            ..
        }] if environment.is_empty()
    ));
    assert!(network.state.read().await.callsigns.is_empty());
}

#[tokio::test]
async fn vatsim_login_requires_revision_100_after_identification() {
    let network = network();
    network
        .register(
            ConnectionId(1),
            "127.0.0.1:1000".parse().unwrap(),
            ProtocolDialect::Vatsim,
        )
        .await
        .unwrap();
    network
        .execute(ConnectionId(1), vatsim_identification("ECP1", Some("CID1")))
        .await;

    let effects = network.execute(ConnectionId(1), pilot_login("ECP1")).await;
    assert!(effects.close.is_some());
    assert!(matches!(
        effects.deliveries.as_slice(),
        [Delivery {
            event: Event::Error {
                code: ErrorCode::InvalidProtocolRevision,
                ..
            },
            ..
        }]
    ));
    assert_eq!(
        network.snapshot(ConnectionId(1)).await.unwrap().phase,
        SessionPhase::Identified
    );
    assert!(network.state.read().await.callsigns.is_empty());
}

#[tokio::test]
async fn vatsim_login_owns_identified_callsign_and_network_id() {
    for (identified_callsign, identified_network_id, login_callsign, login_network_id) in [
        ("ECP1", Some("CID1"), "ECP2", "CID1"),
        ("ECP1", Some("CID1"), "ECP1", "CID2"),
        ("ECP1", None, "ECP1", "CID1"),
    ] {
        let network = network();
        network
            .register(
                ConnectionId(1),
                "127.0.0.1:1000".parse().unwrap(),
                ProtocolDialect::Vatsim,
            )
            .await
            .unwrap();
        network
            .execute(
                ConnectionId(1),
                vatsim_identification(identified_callsign, identified_network_id),
            )
            .await;

        let effects = network
            .execute(
                ConnectionId(1),
                vatsim_pilot_login(login_callsign, login_network_id),
            )
            .await;
        assert!(effects.close.is_some());
        assert!(matches!(
            effects.deliveries.as_slice(),
            [Delivery {
                event: Event::Error {
                    code: ErrorCode::InvalidSource,
                    ..
                },
                ..
            }]
        ));
        assert!(network.state.read().await.callsigns.is_empty());
    }
}

#[tokio::test]
async fn vatsim_pilot_login_emits_capabilities_ip_and_no_flight_plan_profile() {
    let network = network();
    network
        .register(
            ConnectionId(1),
            "127.0.0.42:4321".parse().unwrap(),
            ProtocolDialect::Vatsim,
        )
        .await
        .unwrap();
    network
        .execute(ConnectionId(1), vatsim_identification("ECP1", Some("CID1")))
        .await;

    let effects = network
        .execute(ConnectionId(1), vatsim_pilot_login("ECP1", "CID1"))
        .await;
    assert_eq!(effects.deliveries.len(), 4);
    assert!(matches!(effects.deliveries[0].event, Event::Welcome { .. }));
    assert!(matches!(
        &effects.deliveries[1].event,
        Event::Query {
            source,
            destination: Destination::Direct(destination),
            kind: QueryKind::Capabilities,
            arguments,
        } if source == "SERVER" && destination.as_str() == "ECP1" && arguments.is_empty()
    ));
    assert!(matches!(
        &effects.deliveries[2].event,
        Event::Response {
            source,
            destination: Destination::Direct(destination),
            kind: QueryKind::Raw(kind),
            arguments,
        } if source == "SERVER"
            && destination.as_str() == "ECP1"
            && kind == "IP"
            && arguments == &["127.0.0.42"]
    ));
    assert!(matches!(
        &effects.deliveries[3].event,
        Event::Error {
            callsign: Some(callsign),
            code: ErrorCode::NoFlightPlan,
            environment,
            ..
        } if callsign.as_str() == "ECP1" && environment == "ECP1"
    ));
}

#[tokio::test]
async fn vatsim_atc_login_emits_complete_controller_profile() {
    let network = network();
    network
        .register(
            ConnectionId(1),
            "127.0.0.43:4321".parse().unwrap(),
            ProtocolDialect::Vatsim,
        )
        .await
        .unwrap();
    network
        .execute(
            ConnectionId(1),
            vatsim_identification("ZSPD_TWR", Some("CID2")),
        )
        .await;

    let effects = network
        .execute(ConnectionId(1), vatsim_atc_login("ZSPD_TWR", "CID2"))
        .await;
    assert_eq!(effects.deliveries.len(), 5);
    assert!(matches!(effects.deliveries[0].event, Event::Welcome { .. }));
    assert!(matches!(
        &effects.deliveries[1].event,
        Event::Query {
            source,
            kind: QueryKind::Capabilities,
            arguments,
            ..
        } if source == "SERVER" && arguments.is_empty()
    ));
    assert!(matches!(
        &effects.deliveries[2].event,
        Event::Response {
            source,
            kind: QueryKind::Raw(kind),
            arguments,
            ..
        } if source == "SERVER"
            && kind == "ATC"
            && arguments == &["N", "ZSPD_TWR"]
    ));
    assert!(matches!(
        &effects.deliveries[3].event,
        Event::Response {
            source,
            kind: QueryKind::Capabilities,
            arguments,
            ..
        } if source == "SERVER" && arguments == &["ATCINFO=1", "SECPOS=1"]
    ));
    assert!(matches!(
        &effects.deliveries[4].event,
        Event::Response {
            source,
            kind: QueryKind::Raw(kind),
            arguments,
            ..
        } if source == "SERVER" && kind == "IP" && arguments == &["127.0.0.43"]
    ));
}
