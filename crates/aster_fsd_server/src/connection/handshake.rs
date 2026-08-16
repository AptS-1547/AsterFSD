use crate::runtime::Runtime;
use aster_fsd_codec::FsdFrameCodec;
use aster_fsd_core::RegisterError;
use aster_fsd_model::{ConnectionId, ErrorCode, Event, ProtocolDialect};
use aster_fsd_protocol::{EncodeContext, HandshakeContext, ProtocolBackend};
use futures_util::SinkExt;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio_util::codec::Framed;

pub(super) async fn register_connection(
    runtime: &Runtime,
    framed: &mut Framed<TcpStream, FsdFrameCodec>,
    backend: &dyn ProtocolBackend,
    connection_id: ConnectionId,
    peer: SocketAddr,
    dialect: ProtocolDialect,
) -> bool {
    match runtime.network.register(connection_id, peer, dialect).await {
        Ok(()) => true,
        Err(RegisterError::ServerFull) => {
            let event = Event::Error {
                callsign: None,
                code: ErrorCode::ServerFull,
                environment: String::new(),
                description: ErrorCode::ServerFull.description().to_string(),
            };
            let context = EncodeContext {
                connection_id,
                recipient: None,
                server_name: runtime.config.server_name.clone(),
            };
            if let Ok(frames) = backend.encode(&context, &event) {
                for frame in frames {
                    let _ = framed.send(frame.into_bytes()).await;
                }
            }
            false
        }
        Err(RegisterError::DuplicateConnection) => {
            tracing::error!(%connection_id, "Connection ID collision");
            false
        }
    }
}

pub(super) async fn send_handshake(
    runtime: &Runtime,
    framed: &mut Framed<TcpStream, FsdFrameCodec>,
    backend: &dyn ProtocolBackend,
    context: &HandshakeContext,
) -> bool {
    let handshake_frames = match backend.initial_frames(context) {
        Ok(encoded) => encoded,
        Err(error) => {
            tracing::error!(connection_id = %context.connection_id, error = %error, "Handshake encoding failed");
            let effects = runtime
                .network
                .disconnect(context.connection_id, "handshake encoding failed")
                .await;
            runtime.dispatch(effects).await;
            return false;
        }
    };
    tracing::debug!(
        connection_id = %context.connection_id,
        dialect = ?backend.dialect(),
        frames = handshake_frames.len(),
        wire_bytes = handshake_frames
            .iter()
            .map(|frame| frame.as_bytes().len())
            .sum::<usize>(),
        "Prepared protocol handshake"
    );
    for frame in handshake_frames {
        if let Err(error) = framed.send(frame.into_bytes()).await {
            tracing::debug!(connection_id = %context.connection_id, error = %error, "Handshake write failed");
            let effects = runtime
                .network
                .disconnect(context.connection_id, "handshake write failed")
                .await;
            runtime.dispatch(effects).await;
            return false;
        }
    }
    true
}
