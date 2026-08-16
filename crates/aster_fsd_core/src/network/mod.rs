mod authentication;
mod commands;
mod lifecycle;
mod routing;
mod state;
mod weather;

#[cfg(test)]
mod tests;

use crate::{
    CloseConnection, CoreConfig, Delivery, Effects, UnavailableWeatherProvider, WeatherLookup,
    WeatherProvider,
};
use aster_fsd_auth::{AuthError, Authenticator};
use aster_fsd_model::{
    AuthenticatedIdentity, CLASSIC_PROTOCOL_REVISION, Callsign, ClientPresence, ClientType,
    Command, ConnectionId, Destination, ErrorCode, Event, LoginCommand, Position, ProtocolDialect,
    QueryKind, SessionPhase, VATSIM_PROTOCOL_REVISION, WeatherProfile,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

use state::{NetworkState, Session};

pub use state::{RegisterError, SessionSnapshot};

#[derive(Debug, Clone, Copy)]
enum RangePolicy {
    Source,
    Message,
}

/// Shared authoritative state for every protocol listener.
pub struct Network {
    config: CoreConfig,
    authenticator: Arc<dyn Authenticator>,
    weather_provider: Arc<dyn WeatherProvider>,
    state: RwLock<NetworkState>,
}

impl Network {
    /// Creates an empty network using the supplied authentication port.
    #[must_use]
    pub fn new(config: CoreConfig, authenticator: Arc<dyn Authenticator>) -> Self {
        Self::with_weather_provider(config, authenticator, Arc::new(UnavailableWeatherProvider))
    }

    /// Creates an empty network with an explicit asynchronous weather source.
    #[must_use]
    pub fn with_weather_provider(
        config: CoreConfig,
        authenticator: Arc<dyn Authenticator>,
        weather_provider: Arc<dyn WeatherProvider>,
    ) -> Self {
        Self {
            config,
            authenticator,
            weather_provider,
            state: RwLock::new(NetworkState::default()),
        }
    }

    /// Registers a newly accepted connection before any protocol command runs.
    ///
    /// # Errors
    ///
    /// Returns [`RegisterError::DuplicateConnection`] when the ID already
    /// exists or [`RegisterError::ServerFull`] at the configured capacity.
    pub async fn register(
        &self,
        connection_id: ConnectionId,
        peer: SocketAddr,
        dialect: ProtocolDialect,
    ) -> Result<(), RegisterError> {
        let mut state = self.state.write().await;
        if state.sessions.contains_key(&connection_id) {
            return Err(RegisterError::DuplicateConnection);
        }
        if state.sessions.len() >= self.config.max_clients {
            return Err(RegisterError::ServerFull);
        }
        state.sessions.insert(
            connection_id,
            Session {
                connection_id,
                peer,
                dialect,
                phase: SessionPhase::Connected,
                generation: 0,
                identification: None,
                presence: None,
                position: None,
                flight_plan: None,
            },
        );
        tracing::debug!(
            %connection_id,
            %peer,
            ?dialect,
            registered_clients = state.sessions.len(),
            max_clients = self.config.max_clients,
            "Connection registered in network core"
        );
        Ok(())
    }

    /// Returns a point-in-time snapshot of a registered connection.
    pub async fn snapshot(&self, connection_id: ConnectionId) -> Option<SessionSnapshot> {
        self.state
            .read()
            .await
            .sessions
            .get(&connection_id)
            .map(Session::snapshot)
    }

    /// Broadcasts a server-originated event to all active sessions.
    pub async fn broadcast(&self, event: Event) -> Effects {
        let state = self.state.read().await;
        Self::send(Self::active_ids(&state, None), event)
    }

    /// Executes a decoded command against authoritative state.
    pub async fn execute(&self, connection_id: ConnectionId, command: Command) -> Effects {
        tracing::debug!(
            %connection_id,
            command = command.kind(),
            source = command.source().map_or("", Callsign::as_str),
            "Executing network command"
        );
        let effects = match command {
            Command::Identify(command) => self.identify(connection_id, command).await,
            Command::Login(command) => self.login(connection_id, command).await,
            Command::WeatherRequest {
                source,
                station,
                parsed,
            } => {
                self.weather_request(connection_id, source, station, parsed)
                    .await
            }
            command => self.execute_active(connection_id, command).await,
        };
        tracing::debug!(
            %connection_id,
            deliveries = effects.deliveries.len(),
            recipients = effects
                .deliveries
                .iter()
                .map(|delivery| delivery.recipients.len())
                .sum::<usize>(),
            closes_connection = effects.close.is_some(),
            "Network command executed"
        );
        effects
    }
}
