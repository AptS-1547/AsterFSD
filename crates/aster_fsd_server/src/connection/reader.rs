use crate::runtime::Runtime;
use aster_fsd_codec::FsdFrameCodec;
use aster_fsd_core::{Delivery, Effects};
use aster_fsd_model::{ConnectionId, ErrorCode, Event, SessionPhase};
use aster_fsd_protocol::{DecodeContext, ProtocolBackend, ProtocolErrorKind};
use futures_util::StreamExt;
use std::time::Instant;
use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use tokio_util::sync::CancellationToken;

pub(super) struct ReaderContext<'a> {
    pub(super) runtime: &'a Runtime,
    pub(super) backend: &'a dyn ProtocolBackend,
    pub(super) connection_id: ConnectionId,
    pub(super) challenge: &'a str,
    pub(super) idle_timeout_seconds: u64,
    pub(super) close: &'a CancellationToken,
    pub(super) shutdown: &'a CancellationToken,
}

async fn process_frame(
    runtime: &Runtime,
    backend: &dyn ProtocolBackend,
    connection_id: ConnectionId,
    challenge: &str,
    frame: &[u8],
) -> bool {
    let decode_started = Instant::now();
    let snapshot = runtime.network.snapshot(connection_id).await;
    let phase = snapshot
        .as_ref()
        .map_or(SessionPhase::Closed, |session| session.phase);
    tracing::debug!(
        %connection_id,
        dialect = ?backend.dialect(),
        ?phase,
        direction = "inbound",
        wire_bytes = frame.len(),
        "Received protocol frame"
    );
    let context = DecodeContext {
        connection_id,
        phase,
        callsign: snapshot.and_then(|session| session.callsign),
        challenge: challenge.to_string(),
    };
    match backend.decode(&context, frame) {
        Ok(command) => {
            tracing::debug!(
                %connection_id,
                dialect = ?backend.dialect(),
                command = command.kind(),
                source = command.source().map_or("", aster_fsd_model::Callsign::as_str),
                destination = ?command.destination(),
                direct_target = command
                    .direct_target()
                    .map_or("", aster_fsd_model::Callsign::as_str),
                decode_elapsed = ?decode_started.elapsed(),
                "Decoded protocol command"
            );
            let effects = runtime.network.execute(connection_id, command).await;
            runtime.dispatch(effects).await;
            true
        }
        Err(error) => {
            let code = error.error_code.unwrap_or(ErrorCode::Syntax);
            let closes_connection = matches!(
                error.kind,
                ProtocolErrorKind::Framing | ProtocolErrorKind::Version
            ) || (phase != SessionPhase::Active
                && matches!(
                    code,
                    ErrorCode::InvalidCallsign | ErrorCode::InvalidProtocolRevision
                ));
            tracing::warn!(
                %connection_id,
                kind = ?error.kind,
                error_code = code as u16,
                closes_connection,
                error = %error,
                decode_elapsed = ?decode_started.elapsed(),
                "Protocol decode failed"
            );
            runtime
                .dispatch(Effects {
                    deliveries: vec![Delivery {
                        recipients: vec![connection_id],
                        event: Event::Error {
                            callsign: context.callsign,
                            code,
                            environment: String::new(),
                            description: code.description().to_string(),
                        },
                    }],
                    close: None,
                })
                .await;
            !closes_connection
        }
    }
}

pub(super) async fn read_connection(
    context: ReaderContext<'_>,
    mut stream: futures_util::stream::SplitStream<Framed<TcpStream, FsdFrameCodec>>,
) {
    let idle_timeout =
        tokio::time::sleep(std::time::Duration::from_secs(context.idle_timeout_seconds));
    tokio::pin!(idle_timeout);
    loop {
        tokio::select! {
            () = context.shutdown.cancelled() => break,
            () = context.close.cancelled() => break,
            () = &mut idle_timeout => {
                tracing::info!(connection_id = %context.connection_id, "Client idle timeout elapsed");
                break;
            }
            frame = stream.next() => {
                let Some(frame) = frame else { break; };
                let frame = match frame {
                    Ok(frame) => frame,
                    Err(error) => {
                        tracing::warn!(connection_id = %context.connection_id, error = %error, "FSD framing failed");
                        break;
                    }
                };
                idle_timeout.as_mut().reset(
                    tokio::time::Instant::now()
                        + std::time::Duration::from_secs(context.idle_timeout_seconds),
                );
                if !process_frame(
                    context.runtime,
                    context.backend,
                    context.connection_id,
                    context.challenge,
                    &frame,
                ).await {
                    break;
                }
            }
        }
    }
}
