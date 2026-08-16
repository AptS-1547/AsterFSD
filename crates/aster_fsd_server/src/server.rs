use crate::backend_registry::BackendRegistry;
use crate::config::{ListenerConfig, ServerConfig};
use crate::error::ServerError;
use crate::listener::accept_loop;
use crate::runtime::{Runtime, run_maintenance};
use aster_fsd_core::Network;
use aster_fsd_model::ConnectionId;
use aster_fsd_protocol::ProtocolBackend;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, atomic::AtomicU64};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// Unbound server composition containing runtime state and backend registry.
pub struct Server {
    runtime: Arc<Runtime>,
    registry: BackendRegistry,
}

/// Server whose listeners have all bound successfully.
pub struct BoundServer {
    runtime: Arc<Runtime>,
    listeners: Vec<(ListenerConfig, TcpListener, Arc<dyn ProtocolBackend>)>,
    local_addresses: Vec<SocketAddr>,
}

impl BoundServer {
    /// Returns actual bound addresses, including operating-system assigned ports.
    #[must_use]
    pub fn local_addresses(&self) -> &[SocketAddr] {
        &self.local_addresses
    }

    /// Runs listener and maintenance tasks until cancellation or task failure.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] when a supervised listener or maintenance task
    /// exits unexpectedly. Shutdown waits for every listener task to converge.
    pub async fn serve(self, shutdown: CancellationToken) -> Result<(), ServerError> {
        let mut listeners = JoinSet::new();
        for (config, listener, backend) in self.listeners {
            let runtime = self.runtime.clone();
            let shutdown = shutdown.child_token();
            listeners.spawn(async move {
                accept_loop(runtime, config, listener, backend, shutdown).await
            });
        }

        let maintenance_runtime = self.runtime.clone();
        let maintenance_shutdown = shutdown.child_token();
        let maintenance = tokio::spawn(run_maintenance(maintenance_runtime, maintenance_shutdown));

        tokio::select! {
            () = shutdown.cancelled() => {}
            outcome = listeners.join_next() => {
                match outcome {
                    Some(Ok(result)) => result?,
                    Some(Err(error)) => return Err(ServerError::ListenerTask(error.to_string())),
                    None => {}
                }
            }
        }
        shutdown.cancel();
        while let Some(outcome) = listeners.join_next().await {
            match outcome {
                Ok(Ok(())) => {}
                Ok(Err(error)) => return Err(error),
                Err(error) if error.is_cancelled() => {}
                Err(error) => return Err(ServerError::ListenerTask(error.to_string())),
            }
        }
        if let Err(error) = maintenance.await
            && !error.is_cancelled()
        {
            return Err(ServerError::ListenerTask(error.to_string()));
        }
        Ok(())
    }
}

impl Server {
    /// Validates runtime limits and builds an unbound server.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] for zero capacities or intervals and for an
    /// empty listener set.
    pub fn new(
        config: ServerConfig,
        network: Arc<Network>,
        registry: BackendRegistry,
    ) -> Result<Self, ServerError> {
        if config.mailbox_capacity == 0 {
            return Err(ServerError::InvalidMailboxCapacity);
        }
        if config.listeners.is_empty() {
            return Err(ServerError::NoListeners);
        }
        if config.wind_delta_interval_seconds == 0
            || config
                .listeners
                .iter()
                .any(|listener| listener.idle_timeout_seconds == 0)
        {
            return Err(ServerError::InvalidInterval);
        }
        Ok(Self {
            runtime: Arc::new(Runtime {
                config,
                network,
                connections: RwLock::new(HashMap::<ConnectionId, _>::new()),
                next_connection_id: AtomicU64::new(1),
            }),
            registry,
        })
    }

    /// Binds every configured listener before exposing the server as runnable.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::MissingBackend`] for an unregistered dialect or
    /// [`ServerError::ListenerIo`] when any address cannot be bound or queried.
    pub async fn bind(self) -> Result<BoundServer, ServerError> {
        let mut listeners = Vec::new();
        let mut local_addresses = Vec::new();
        for config in &self.runtime.config.listeners {
            let backend =
                self.registry
                    .get(config.protocol)
                    .ok_or_else(|| ServerError::MissingBackend {
                        listener: config.name.clone(),
                    })?;
            let listener = TcpListener::bind(format!("{}:{}", config.address, config.port))
                .await
                .map_err(|source| ServerError::ListenerIo {
                    listener: config.name.clone(),
                    source,
                })?;
            local_addresses.push(listener.local_addr().map_err(|source| {
                ServerError::ListenerIo {
                    listener: config.name.clone(),
                    source,
                }
            })?);
            listeners.push((config.clone(), listener, backend));
        }
        Ok(BoundServer {
            runtime: self.runtime,
            listeners,
            local_addresses,
        })
    }
}
