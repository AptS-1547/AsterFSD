use super::{atc_login, network, pilot_login};
use crate::{CloseConnection, CoreConfig, Delivery, Network, RegisterError};
use aster_fsd_auth::AllowAllAuthenticator;
use aster_fsd_model::{
    Callsign, Command, ConnectionId, Destination, ErrorCode, Event, ProtocolDialect, SessionPhase,
};
use std::sync::Arc;

#[tokio::test]
async fn disconnect_releases_both_indexes_once() {
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
    assert!(!network.disconnect(ConnectionId(1), "EOF").await.is_empty());
    assert!(network.disconnect(ConnectionId(1), "EOF").await.is_empty());
    network
        .register(
            ConnectionId(2),
            "127.0.0.1:1001".parse().unwrap(),
            ProtocolDialect::Classic,
        )
        .await
        .unwrap();
    let effects = network.execute(ConnectionId(2), pilot_login("ECP1")).await;
    assert!(!effects.deliveries.iter().any(|effect| matches!(
        effect,
        Delivery { event, .. }
            if matches!(event, Event::Error {
                code: ErrorCode::CallsignInUse,
                ..
            })
    )));
}

#[tokio::test]
async fn registration_capacity_and_prelogin_commands_are_bounded() {
    let network = Network::new(
        CoreConfig {
            max_clients: 1,
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
    assert_eq!(
        network
            .register(
                ConnectionId(2),
                "127.0.0.1:1001".parse().unwrap(),
                ProtocolDialect::Classic,
            )
            .await,
        Err(RegisterError::ServerFull)
    );
    let effects = network
        .execute(
            ConnectionId(1),
            Command::Text {
                source: Callsign::parse("ECP1").unwrap(),
                destination: Destination::All,
                message: "prelogin".to_string(),
            },
        )
        .await;
    assert!(effects.is_empty());
    assert_eq!(
        network.snapshot(ConnectionId(1)).await.unwrap().phase,
        SessionPhase::Connected
    );
}

#[tokio::test]
async fn c_kill_checks_target_before_rating_and_notifies_requester_before_close() {
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
    network
        .execute(ConnectionId(1), atc_login("ECP1", 11))
        .await;
    network.execute(ConnectionId(2), pilot_login("ECP2")).await;

    let unknown = network
        .execute(
            ConnectionId(2),
            Command::Kill {
                source: Callsign::parse("ECP2").unwrap(),
                target: Callsign::parse("MISSING").unwrap(),
                reason: "reason".to_string(),
            },
        )
        .await;
    assert!(matches!(
        unknown.deliveries.as_slice(),
        [Delivery {
            event: Event::Error {
                code: ErrorCode::NoSuchCallsign,
                environment,
                ..
            },
            ..
        }] if environment == "MISSING"
    ));

    let denied = network
        .execute(
            ConnectionId(2),
            Command::Kill {
                source: Callsign::parse("ECP2").unwrap(),
                target: Callsign::parse("ECP1").unwrap(),
                reason: "reason".to_string(),
            },
        )
        .await;
    assert!(matches!(
        denied.deliveries.as_slice(),
        [Delivery {
            recipients,
            event: Event::Welcome { message, .. },
        }] if recipients == &[ConnectionId(2)]
            && message == "You are not allowed to kill users!"
    ));

    let accepted = network
        .execute(
            ConnectionId(1),
            Command::Kill {
                source: Callsign::parse("ECP1").unwrap(),
                target: Callsign::parse("ECP2").unwrap(),
                reason: "network abuse".to_string(),
            },
        )
        .await;
    assert!(matches!(
        accepted.deliveries.as_slice(),
        [
            Delivery {
                recipients: notice_recipients,
                event: Event::Welcome { message, .. },
            },
            Delivery {
                recipients: target_recipients,
                event: Event::Disconnect { target, reason },
            },
            Delivery {
                recipients: removal_recipients,
                event: Event::ClientRemoved { callsign, .. },
            },
        ] if notice_recipients == &[ConnectionId(1)]
            && message == "Attempting to kill ECP2"
            && target_recipients == &[ConnectionId(2)]
            && target.as_str() == "ECP2"
            && reason == "network abuse"
            && removal_recipients == &[ConnectionId(1)]
            && callsign.as_str() == "ECP2"
    ));
    assert!(matches!(
        accepted.close,
        Some(CloseConnection {
            connection_id: ConnectionId(2),
            ref reason,
        }) if reason == "network abuse"
    ));
    assert!(network.snapshot(ConnectionId(2)).await.is_none());
}
