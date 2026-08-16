use super::{pilot_login, weather_network, weather_profile};
use crate::{Delivery, WeatherObservation, WeatherProviderError};
use aster_fsd_model::{Callsign, Command, ConnectionId, ErrorCode, Event, ProtocolDialect};

#[tokio::test]
async fn weather_provider_selects_parsed_and_raw_responses_for_the_requester() {
    let profile = weather_profile();
    let network = weather_network(Ok(Some(WeatherObservation {
        raw_metar: Some("KJFK 161651Z 18012KT 10SM FEW030 15/08 A2992".to_string()),
        profile: Some(profile.clone()),
    })));
    network
        .register(
            ConnectionId(1),
            "127.0.0.1:1000".parse().unwrap(),
            ProtocolDialect::Classic,
        )
        .await
        .unwrap();
    network.execute(ConnectionId(1), pilot_login("ECP1")).await;

    let parsed = network
        .execute(
            ConnectionId(1),
            Command::WeatherRequest {
                source: Callsign::parse("ECP1").unwrap(),
                station: "KJFK".to_string(),
                parsed: true,
            },
        )
        .await;
    assert!(matches!(
        parsed.deliveries.as_slice(),
        [Delivery {
            recipients,
            event: Event::WeatherProfile {
                destination,
                station,
                profile: actual,
            },
        }] if recipients == &[ConnectionId(1)]
            && destination.as_str() == "ECP1"
            && station == "KJFK"
            && actual == &profile
    ));

    let raw = network
        .execute(
            ConnectionId(1),
            Command::WeatherRequest {
                source: Callsign::parse("ECP1").unwrap(),
                station: "KJFK".to_string(),
                parsed: false,
            },
        )
        .await;
    assert!(matches!(
        raw.deliveries.as_slice(),
        [Delivery {
            recipients,
            event: Event::WeatherReport {
                source,
                destination,
                station,
                report,
            },
        }] if recipients == &[ConnectionId(1)]
            && source == "server"
            && destination.as_str() == "ECP1"
            && station == "KJFK"
            && report.starts_with("KJFK ")
    ));
}

#[tokio::test]
async fn weather_requests_enforce_source_ownership_and_hide_provider_failures() {
    let network = weather_network(Err(WeatherProviderError::new("upstream details")));
    network
        .register(
            ConnectionId(1),
            "127.0.0.1:1000".parse().unwrap(),
            ProtocolDialect::Classic,
        )
        .await
        .unwrap();
    network.execute(ConnectionId(1), pilot_login("ECP1")).await;

    let spoof = network
        .execute(
            ConnectionId(1),
            Command::WeatherRequest {
                source: Callsign::parse("ECP2").unwrap(),
                station: "KJFK".to_string(),
                parsed: true,
            },
        )
        .await;
    assert!(matches!(
        spoof.deliveries[0].event,
        Event::Error {
            code: ErrorCode::InvalidSource,
            ..
        }
    ));

    let failure = network
        .execute(
            ConnectionId(1),
            Command::WeatherRequest {
                source: Callsign::parse("ECP1").unwrap(),
                station: "KJFK".to_string(),
                parsed: true,
            },
        )
        .await;
    assert!(matches!(
        &failure.deliveries[0].event,
        Event::Error {
            code: ErrorCode::NoWeather,
            environment,
            ..
        } if environment == "KJFK"
    ));
}
