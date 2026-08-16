use crate::config::ListenerConfig;
use crate::connection::handle_connection;
use crate::error::ServerError;
use crate::runtime::Runtime;
use aster_fsd_protocol::ProtocolBackend;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub(crate) async fn accept_loop(
    runtime: Arc<Runtime>,
    config: ListenerConfig,
    listener: TcpListener,
    backend: Arc<dyn ProtocolBackend>,
    shutdown: CancellationToken,
) -> Result<(), ServerError> {
    tracing::info!(
        listener = %config.name,
        protocol = ?config.protocol,
        address = %listener.local_addr().map_err(|source| ServerError::ListenerIo {
            listener: config.name.clone(),
            source,
        })?,
        "FSD listener ready"
    );
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            accepted = listener.accept() => {
                let (stream, peer) = accepted.map_err(|source| ServerError::ListenerIo {
                    listener: config.name.clone(),
                    source,
                })?;
                tracing::debug!(
                    listener = %config.name,
                    protocol = ?config.protocol,
                    %peer,
                    "Accepted TCP connection"
                );
                let runtime = runtime.clone();
                let backend = backend.clone();
                let config = config.clone();
                let shutdown = shutdown.child_token();
                connections.spawn(async move {
                    handle_connection(runtime, config, stream, peer, backend, shutdown).await;
                });
            }
            Some(outcome) = connections.join_next(), if !connections.is_empty() => {
                if let Err(error) = outcome {
                    tracing::error!(error = %error, "FSD connection task failed");
                }
            }
        }
    }
    shutdown.cancel();
    while let Some(outcome) = connections.join_next().await {
        if let Err(error) = outcome
            && !error.is_cancelled()
        {
            tracing::error!(error = %error, "FSD connection task failed during shutdown");
        }
    }
    Ok(())
}
