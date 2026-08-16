use super::{atc_login, atc_position, flight_plan, network, pilot_login, pilot_position};
use crate::Delivery;
use aster_fsd_model::{
    Callsign, Command, ConnectionId, Destination, ErrorCode, Event, ProtocolDialect, QueryKind,
};

#[tokio::test]
async fn flight_plan_delivery_is_atc_only_and_bounded_to_four_hundred_nm() {
    let network = network();
    for id in [
        ConnectionId(1),
        ConnectionId(2),
        ConnectionId(3),
        ConnectionId(4),
    ] {
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
        .execute(ConnectionId(2), atc_login("ATCNEAR", 5))
        .await;
    network
        .execute(ConnectionId(3), pilot_login("PILOT2"))
        .await;
    network
        .execute(ConnectionId(4), atc_login("ATCFAR", 5))
        .await;
    network
        .execute(ConnectionId(1), pilot_position("PILOT1", 0.0, 0.0, 0))
        .await;
    network
        .execute(ConnectionId(2), atc_position("ATCNEAR", 1.0, 0.0, 6, 400))
        .await;
    network
        .execute(ConnectionId(3), pilot_position("PILOT2", 1.0, 0.0, 0))
        .await;
    network
        .execute(ConnectionId(4), atc_position("ATCFAR", 7.0, 0.0, 6, 400))
        .await;

    let effects = network
        .execute(ConnectionId(1), Command::FlightPlan(flight_plan("PILOT1")))
        .await;
    assert!(matches!(
        effects.deliveries.as_slice(),
        [Delivery { recipients, event: Event::FlightPlan { .. } }]
            if recipients == &[ConnectionId(2)]
    ));
}

#[tokio::test]
async fn unknown_direct_destination_produces_no_delivery() {
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
                source: Callsign::parse("ECP1").unwrap(),
                destination: Destination::Direct(Callsign::parse("MISSING").unwrap()),
                message: "nobody".to_string(),
            },
        )
        .await;
    assert!(effects.is_empty());
}

#[tokio::test]
async fn typed_handoff_and_client_data_follow_c_direct_relay_semantics() {
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

    let handoff = network
        .execute(
            ConnectionId(1),
            Command::Handoff {
                source: Callsign::parse("ECP1").unwrap(),
                target: Callsign::parse("ECP2").unwrap(),
                kind: aster_fsd_model::HandoffKind::Request,
                fields: vec!["ECP3".to_string()],
            },
        )
        .await;
    assert!(matches!(
        handoff.deliveries.as_slice(),
        [Delivery {
            recipients,
            event: Event::Handoff {
                source,
                target,
                fields,
                ..
            },
        }] if recipients == &[ConnectionId(2)]
            && source.as_str() == "ECP1"
            && target.as_str() == "ECP2"
            && fields == &["ECP3"]
    ));

    let client_data = network
        .execute(
            ConnectionId(2),
            Command::ClientData {
                source: Callsign::parse("ECP2").unwrap(),
                target: Callsign::parse("ECP1").unwrap(),
                kind: aster_fsd_model::ClientDataKind::CommunicationReply,
                fields: vec!["122.800".to_string()],
            },
        )
        .await;
    assert!(matches!(
        client_data.deliveries.as_slice(),
        [Delivery {
            recipients,
            event: Event::ClientData { target, fields, .. },
        }] if recipients == &[ConnectionId(1)]
            && target.as_str() == "ECP1"
            && fields == &["122.800"]
    ));

    let unknown = network
        .execute(
            ConnectionId(1),
            Command::Handoff {
                source: Callsign::parse("ECP1").unwrap(),
                target: Callsign::parse("MISSING").unwrap(),
                kind: aster_fsd_model::HandoffKind::Accept,
                fields: vec!["ECP3".to_string()],
            },
        )
        .await;
    assert!(unknown.is_empty());
}

#[tokio::test]
async fn c_query_routing_uses_source_range_while_text_uses_message_range() {
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
    network
        .execute(ConnectionId(1), pilot_position("ECP1", 0.0, 0.0, 0))
        .await;
    network
        .execute(ConnectionId(2), pilot_position("ECP2", 0.8, 0.0, 40_000))
        .await;

    let query = network
        .execute(
            ConnectionId(1),
            Command::Query {
                source: Callsign::parse("ECP1").unwrap(),
                destination: Destination::Range("@94836".to_string()),
                kind: QueryKind::AircraftConfiguration,
                arguments: Vec::new(),
            },
        )
        .await;
    assert!(query.is_empty());

    let text = network
        .execute(
            ConnectionId(1),
            Command::Text {
                source: Callsign::parse("ECP1").unwrap(),
                destination: Destination::Range("@94836".to_string()),
                message: "within combined pilot range".to_string(),
            },
        )
        .await;
    assert!(matches!(
        text.deliveries.as_slice(),
        [Delivery { recipients, .. }] if recipients == &[ConnectionId(2)]
    ));
}

#[tokio::test]
async fn c_server_flight_plan_query_is_direct_and_other_server_queries_are_silent() {
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
    network
        .execute(ConnectionId(1), Command::FlightPlan(flight_plan("ECP1")))
        .await;

    let response = network
        .execute(
            ConnectionId(2),
            Command::Query {
                source: Callsign::parse("ECP2").unwrap(),
                destination: Destination::Server,
                kind: QueryKind::FlightPlan,
                arguments: vec!["ECP1".to_string()],
            },
        )
        .await;
    assert!(matches!(
        response.deliveries.as_slice(),
        [Delivery {
            recipients,
            event: Event::FlightPlan {
                plan,
                destination: Destination::Direct(target),
            },
        }] if recipients == &[ConnectionId(2)]
            && plan.callsign.as_str() == "ECP1"
            && target.as_str() == "ECP2"
    ));

    let unsupported = network
        .execute(
            ConnectionId(2),
            Command::Query {
                source: Callsign::parse("ECP2").unwrap(),
                destination: Destination::Server,
                kind: QueryKind::Capabilities,
                arguments: Vec::new(),
            },
        )
        .await;
    assert!(unsupported.is_empty());

    let missing_plan = network
        .execute(
            ConnectionId(1),
            Command::Query {
                source: Callsign::parse("ECP1").unwrap(),
                destination: Destination::Server,
                kind: QueryKind::FlightPlan,
                arguments: vec!["ECP2".to_string()],
            },
        )
        .await;
    assert!(matches!(
        missing_plan.deliveries.as_slice(),
        [Delivery {
            event: Event::Error {
                code: ErrorCode::NoFlightPlan,
                environment,
                ..
            },
            ..
        }] if environment.is_empty()
    ));
}
