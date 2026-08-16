mod handshake;
mod reader;
pub(crate) mod writer;

use crate::challenge::challenge;
use crate::config::ListenerConfig;
use crate::runtime::{ConnectionHandle, Runtime};
use aster_fsd_codec::FsdFrameCodec;
use aster_fsd_model::ConnectionId;
use aster_fsd_protocol::{HandshakeContext, ProtocolBackend};
use futures_util::StreamExt;
use std::net::SocketAddr;
use std::sync::{Arc, atomic::Ordering};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_util::codec::Framed;
use tokio_util::sync::CancellationToken;

use self::handshake::{register_connection, send_handshake};
use self::reader::{ReaderContext, read_connection};
use self::writer::spawn_writer;

pub(crate) async fn handle_connection(
    runtime: Arc<Runtime>,
    listener: ListenerConfig,
    stream: TcpStream,
    peer: SocketAddr,
    backend: Arc<dyn ProtocolBackend>,
    shutdown: CancellationToken,
) {
    let connection_id = ConnectionId(runtime.next_connection_id.fetch_add(1, Ordering::Relaxed));
    let challenge = challenge();
    let codec = match FsdFrameCodec::new(listener.max_frame_bytes) {
        Ok(codec) => codec,
        Err(error) => {
            tracing::error!(listener = %listener.name, error = %error, "Invalid listener frame limit");
            return;
        }
    };
    tracing::debug!(
        %connection_id,
        listener = %listener.name,
        protocol = ?listener.protocol,
        max_frame_bytes = listener.max_frame_bytes,
        idle_timeout_seconds = listener.idle_timeout_seconds,
        "Initialized connection transport"
    );
    let mut framed = Framed::new(stream, codec);

    if !register_connection(
        &runtime,
        &mut framed,
        backend.as_ref(),
        connection_id,
        peer,
        listener.protocol,
    )
    .await
    {
        return;
    }

    let handshake = HandshakeContext {
        connection_id,
        peer,
        server_name: runtime.config.server_name.clone(),
        server_version: runtime.config.server_version.clone(),
        challenge: challenge.clone(),
    };
    if !send_handshake(&runtime, &mut framed, backend.as_ref(), &handshake).await {
        return;
    }

    let (sink, stream) = framed.split();
    let (outbound, receiver) = mpsc::channel(runtime.config.mailbox_capacity);
    let close = CancellationToken::new();
    runtime.connections.write().await.insert(
        connection_id,
        ConnectionHandle {
            backend: backend.clone(),
            outbound,
            cancel: close.clone(),
        },
    );

    tracing::info!(connection_id = %connection_id, %peer, protocol = ?listener.protocol, "Client connected");
    let writer = spawn_writer(connection_id, sink, receiver, close.clone());

    read_connection(
        ReaderContext {
            runtime: &runtime,
            backend: backend.as_ref(),
            connection_id,
            challenge: &challenge,
            idle_timeout_seconds: listener.idle_timeout_seconds,
            close: &close,
            shutdown: &shutdown,
        },
        stream,
    )
    .await;

    runtime.connections.write().await.remove(&connection_id);
    let effects = runtime
        .network
        .disconnect(connection_id, "connection closed")
        .await;
    runtime.dispatch(effects).await;
    close.cancel();
    if let Err(error) = writer.await
        && !error.is_cancelled()
    {
        tracing::warn!(connection_id = %connection_id, error = %error, "Client writer failed");
    }
    tracing::info!(connection_id = %connection_id, %peer, "Client disconnected");
}
