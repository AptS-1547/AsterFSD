use crate::config::ServerConfig;
use crate::connection::writer::Outbound;
use aster_fsd_core::{CloseConnection, Delivery, Effects, Network};
use aster_fsd_model::{ConnectionId, Event, ProtocolDialect};
use aster_fsd_protocol::{EncodeContext, ProtocolBackend, WireFrame};
use std::collections::HashMap;
use std::sync::{Arc, atomic::AtomicU64};
use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub(crate) struct ConnectionHandle {
    pub(crate) backend: Arc<dyn ProtocolBackend>,
    pub(crate) outbound: mpsc::Sender<Outbound>,
    pub(crate) cancel: CancellationToken,
}

pub(crate) struct Runtime {
    pub(crate) config: ServerConfig,
    pub(crate) network: Arc<Network>,
    pub(crate) connections: RwLock<HashMap<ConnectionId, ConnectionHandle>>,
    pub(crate) next_connection_id: AtomicU64,
}

impl Runtime {
    pub(crate) async fn dispatch(&self, effects: Effects) {
        for Delivery { recipients, event } in effects.deliveries {
            tracing::debug!(
                event = event.kind(),
                recipients = recipients.len(),
                "Dispatching network event"
            );
            let mut encoded_by_dialect: HashMap<ProtocolDialect, Arc<[WireFrame]>> = HashMap::new();
            for connection_id in recipients {
                let handle = self.connections.read().await.get(&connection_id).cloned();
                let Some(handle) = handle else {
                    continue;
                };
                let snapshot = self.network.snapshot(connection_id).await;
                let context = EncodeContext {
                    connection_id,
                    recipient: snapshot.and_then(|session| session.presence),
                    server_name: self.config.server_name.clone(),
                };
                let dialect = handle.backend.dialect();
                let frames = if handle.backend.encoding_is_recipient_specific(&event) {
                    tracing::trace!(
                        %connection_id,
                        ?dialect,
                        event = event.kind(),
                        "Encoding recipient-specific event"
                    );
                    handle.backend.encode(&context, &event).map(Arc::from)
                } else if let Some(frames) = encoded_by_dialect.get(&dialect) {
                    tracing::trace!(
                        %connection_id,
                        ?dialect,
                        event = event.kind(),
                        "Reusing dialect frame cache"
                    );
                    Ok(frames.clone())
                } else {
                    tracing::trace!(
                        %connection_id,
                        ?dialect,
                        event = event.kind(),
                        "Encoding event for dialect cache"
                    );
                    handle.backend.encode(&context, &event).map(|frames| {
                        let frames: Arc<[WireFrame]> = Arc::from(frames);
                        encoded_by_dialect.insert(dialect, frames.clone());
                        frames
                    })
                };
                match frames {
                    Ok(frames) => match handle.outbound.try_send(Outbound::Frames(frames)) {
                        Ok(()) => {
                            tracing::trace!(
                                %connection_id,
                                ?dialect,
                                event = event.kind(),
                                "Queued outbound frame batch"
                            );
                        }
                        Err(mpsc::error::TrySendError::Full(_)) => {
                            tracing::warn!(connection_id = %connection_id, "Slow client mailbox is full");
                            handle.cancel.cancel();
                        }
                        Err(mpsc::error::TrySendError::Closed(_)) => {
                            handle.cancel.cancel();
                        }
                    },
                    Err(error) => {
                        tracing::error!(connection_id = %connection_id, error = %error, "Protocol encoding failed");
                        handle.cancel.cancel();
                    }
                }
            }
        }
        if let Some(CloseConnection {
            connection_id,
            reason,
        }) = effects.close
        {
            let handle = self.connections.read().await.get(&connection_id).cloned();
            if let Some(handle) = handle {
                tracing::debug!(connection_id = %connection_id, %reason, "Closing client connection");
                if handle.outbound.try_send(Outbound::Close).is_err() {
                    handle.cancel.cancel();
                }
            }
        }
    }
}

pub(crate) async fn run_maintenance(runtime: Arc<Runtime>, shutdown: CancellationToken) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(
        runtime.config.wind_delta_interval_seconds,
    ));
    interval.tick().await;
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            _ = interval.tick() => {
                let speed = i32::from(rand::random::<u8>() % 11) - 5;
                let direction = i32::from(rand::random::<u8>() % 21) - 10;
                let effects = runtime
                    .network
                    .broadcast(Event::WindDelta { speed, direction })
                    .await;
                runtime.dispatch(effects).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ListenerConfig;
    use aster_fsd_auth::AllowAllAuthenticator;
    use aster_fsd_core::{CoreConfig, Delivery};
    use aster_fsd_protocol_classic::ClassicProtocol;

    #[tokio::test]
    async fn full_mailbox_cancels_only_the_slow_connection() {
        let runtime = Runtime {
            config: ServerConfig {
                server_name: "AsterFSD".to_string(),
                server_version: "0.2.0".to_string(),
                mailbox_capacity: 1,
                wind_delta_interval_seconds: 70,
                listeners: vec![ListenerConfig {
                    name: "classic".to_string(),
                    address: "127.0.0.1".to_string(),
                    port: 0,
                    protocol: ProtocolDialect::Classic,
                    max_frame_bytes: 511,
                    idle_timeout_seconds: 500,
                }],
            },
            network: Arc::new(Network::new(
                CoreConfig::default(),
                Arc::new(AllowAllAuthenticator),
            )),
            connections: RwLock::new(HashMap::new()),
            next_connection_id: AtomicU64::new(3),
        };
        let (slow_sender, mut slow_receiver) = mpsc::channel(1);
        let (healthy_sender, mut healthy_receiver) = mpsc::channel(2);
        let slow_cancel = CancellationToken::new();
        let healthy_cancel = CancellationToken::new();
        runtime.connections.write().await.extend([
            (
                ConnectionId(1),
                ConnectionHandle {
                    backend: Arc::new(ClassicProtocol),
                    outbound: slow_sender,
                    cancel: slow_cancel.clone(),
                },
            ),
            (
                ConnectionId(2),
                ConnectionHandle {
                    backend: Arc::new(ClassicProtocol),
                    outbound: healthy_sender,
                    cancel: healthy_cancel.clone(),
                },
            ),
        ]);

        for speed in [1, 2] {
            runtime
                .dispatch(Effects {
                    deliveries: vec![Delivery {
                        recipients: vec![ConnectionId(1), ConnectionId(2)],
                        event: Event::WindDelta {
                            speed,
                            direction: 0,
                        },
                    }],
                    close: None,
                })
                .await;
        }

        assert!(slow_cancel.is_cancelled());
        assert!(!healthy_cancel.is_cancelled());
        assert!(matches!(slow_receiver.try_recv(), Ok(Outbound::Frames(_))));
        assert!(matches!(
            healthy_receiver.try_recv(),
            Ok(Outbound::Frames(_))
        ));
        assert!(matches!(
            healthy_receiver.try_recv(),
            Ok(Outbound::Frames(_))
        ));
    }
}
