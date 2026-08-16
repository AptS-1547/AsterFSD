use crate::{BackendRegistry, ListenerConfig, Server, ServerConfig, ServerError};
use aster_fsd_auth::{AllowAllAuthenticator, AuthError, Authenticator};
use aster_fsd_core::{
    CoreConfig, Network, WeatherLookup, WeatherObservation, WeatherProvider, WeatherProviderError,
};
use aster_fsd_model::{
    AuthenticatedIdentity, CloudLayer, ProtocolDialect, TemperatureLayer, WeatherProfile, WindLayer,
};
use aster_fsd_protocol_aster::AsterProtocolV1;
use aster_fsd_protocol_classic::ClassicProtocol;
use aster_fsd_protocol_vatsim::VatsimProtocol;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::{Duration, timeout};
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
struct FixtureWeatherProvider;

#[derive(Debug, Clone, Copy)]
pub(super) enum LoginFailure {
    InvalidCredentials,
    Suspended,
}

#[derive(Debug)]
struct FailingAuthenticator(LoginFailure);

#[async_trait]
impl Authenticator for FailingAuthenticator {
    async fn authorize_client(&self, _client_id: &str) -> Result<(), AuthError> {
        Ok(())
    }

    async fn authenticate(
        &self,
        _network_id: &str,
        _password: &str,
    ) -> Result<AuthenticatedIdentity, AuthError> {
        Err(match self.0 {
            LoginFailure::InvalidCredentials => AuthError::InvalidCredentials,
            LoginFailure::Suspended => AuthError::Suspended,
        })
    }
}

#[async_trait]
impl WeatherProvider for FixtureWeatherProvider {
    async fn lookup(
        &self,
        request: &WeatherLookup,
    ) -> Result<Option<WeatherObservation>, WeatherProviderError> {
        if request.station != "KJFK" {
            return Ok(None);
        }
        Ok(Some(WeatherObservation {
            raw_metar: Some("KJFK 161651Z 18012KT 10SM FEW030 15/08 A2992".to_string()),
            profile: Some(WeatherProfile {
                temperatures: [TemperatureLayer {
                    ceiling: 100,
                    temperature: 15,
                }; 4],
                winds: [WindLayer {
                    ceiling: 2_500,
                    floor: 0,
                    direction: 180,
                    speed: 12,
                    gusting: 0,
                    turbulence: 1,
                }; 4],
                clouds: [CloudLayer {
                    ceiling: 5_000,
                    floor: 3_000,
                    coverage: 4,
                    icing: 0,
                    turbulence: 1,
                }; 2],
                thunderstorm: CloudLayer {
                    ceiling: 35_000,
                    floor: 20_000,
                    coverage: 1,
                    icing: 2,
                    turbulence: 3,
                },
                barometer: 2_992,
                visibility: 12.5,
            }),
        }))
    }
}

pub(super) async fn start_server(
    listeners: Vec<ListenerConfig>,
) -> (
    Vec<SocketAddr>,
    CancellationToken,
    tokio::task::JoinHandle<Result<(), ServerError>>,
) {
    start_server_with_authenticator(listeners, Arc::new(AllowAllAuthenticator)).await
}

pub(super) async fn start_server_with_failure(
    listeners: Vec<ListenerConfig>,
    failure: LoginFailure,
) -> (
    Vec<SocketAddr>,
    CancellationToken,
    tokio::task::JoinHandle<Result<(), ServerError>>,
) {
    start_server_with_authenticator(listeners, Arc::new(FailingAuthenticator(failure))).await
}

async fn start_server_with_authenticator(
    listeners: Vec<ListenerConfig>,
    authenticator: Arc<dyn Authenticator>,
) -> (
    Vec<SocketAddr>,
    CancellationToken,
    tokio::task::JoinHandle<Result<(), ServerError>>,
) {
    let network = Arc::new(Network::with_weather_provider(
        CoreConfig::default(),
        authenticator,
        Arc::new(FixtureWeatherProvider),
    ));
    let mut registry = BackendRegistry::default();
    registry.register(Arc::new(ClassicProtocol));
    registry.register(Arc::new(VatsimProtocol::default()));
    registry.register(Arc::new(AsterProtocolV1));
    let server = Server::new(
        ServerConfig {
            server_name: "AsterFSD".to_string(),
            server_version: "0.2.0".to_string(),
            mailbox_capacity: 16,
            wind_delta_interval_seconds: 70,
            listeners,
        },
        network,
        registry,
    )
    .unwrap();
    let bound = server.bind().await.unwrap();
    let addresses = bound.local_addresses().to_vec();
    let shutdown = CancellationToken::new();
    let task_shutdown = shutdown.clone();
    let task = tokio::spawn(bound.serve(task_shutdown));
    (addresses, shutdown, task)
}

pub(super) fn listener(
    name: &str,
    protocol: ProtocolDialect,
    max_frame_bytes: usize,
) -> ListenerConfig {
    ListenerConfig {
        name: name.to_string(),
        address: "127.0.0.1".to_string(),
        port: 0,
        protocol,
        max_frame_bytes,
        idle_timeout_seconds: 500,
    }
}

pub(super) async fn start_classic_server() -> (
    SocketAddr,
    CancellationToken,
    tokio::task::JoinHandle<Result<(), ServerError>>,
) {
    let (addresses, shutdown, task) =
        start_server(vec![listener("classic", ProtocolDialect::Classic, 511)]).await;
    (addresses[0], shutdown, task)
}

pub(super) async fn read_line(reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) -> String {
    let mut line = String::new();
    timeout(Duration::from_secs(2), reader.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    line
}

pub(super) async fn assert_connection_closes(stream: &mut TcpStream) {
    let mut byte = [0_u8; 1];
    let outcome = timeout(Duration::from_secs(3), stream.read(&mut byte))
        .await
        .expect("connection must close before the boundary timeout");
    assert!(
        matches!(outcome, Ok(0) | Err(_)),
        "closed connection produced unexpected bytes: {outcome:?}"
    );
}

pub(super) async fn assert_no_bytes(reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) {
    let mut byte = [0_u8; 1];
    assert!(
        timeout(Duration::from_millis(100), reader.read_exact(&mut byte))
            .await
            .is_err(),
        "connection unexpectedly received byte {byte:?}"
    );
}

pub(super) async fn assert_reader_closes(reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) {
    let mut byte = [0_u8; 1];
    let outcome = timeout(Duration::from_secs(3), reader.read(&mut byte))
        .await
        .expect("connection must close before the boundary timeout");
    assert!(
        matches!(outcome, Ok(0) | Err(_)),
        "closed connection produced unexpected bytes: {outcome:?}"
    );
}
