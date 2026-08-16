use aster_fsd_model::ConnectionId;
use aster_fsd_protocol::WireFrame;
use bytes::Bytes;
use futures_util::SinkExt;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_util::codec::Framed;
use tokio_util::sync::CancellationToken;

use aster_fsd_codec::FsdFrameCodec;

pub(crate) enum Outbound {
    Frames(Arc<[WireFrame]>),
    Close,
}

pub(super) fn spawn_writer(
    connection_id: ConnectionId,
    mut sink: futures_util::stream::SplitSink<Framed<TcpStream, FsdFrameCodec>, Bytes>,
    mut receiver: mpsc::Receiver<Outbound>,
    close: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move { run_writer(connection_id, &mut sink, &mut receiver, &close).await })
}

async fn run_writer<S>(
    connection_id: ConnectionId,
    sink: &mut S,
    receiver: &mut mpsc::Receiver<Outbound>,
    close: &CancellationToken,
) where
    S: futures_util::Sink<Bytes> + Unpin,
    S::Error: std::fmt::Display,
{
    while let Some(message) = receiver.recv().await {
        match message {
            Outbound::Frames(frame_batch) => {
                let wire_bytes = frame_batch
                    .iter()
                    .map(|frame| frame.as_bytes().len())
                    .sum::<usize>();
                tracing::trace!(
                    %connection_id,
                    direction = "outbound",
                    frames = frame_batch.len(),
                    wire_bytes,
                    "Writing outbound frame batch"
                );
                for (frame_index, frame) in frame_batch.iter().enumerate() {
                    if let Err(error) = sink.send(frame.clone().into_bytes()).await {
                        tracing::warn!(
                            %connection_id,
                            direction = "outbound",
                            frame_index,
                            frames = frame_batch.len(),
                            wire_bytes,
                            error = %error,
                            "Client socket write failed"
                        );
                        close.cancel();
                        return;
                    }
                }
            }
            Outbound::Close => {
                if let Err(error) = sink.close().await {
                    tracing::debug!(
                        %connection_id,
                        direction = "outbound",
                        error = %error,
                        "Client socket close failed"
                    );
                }
                close.cancel();
                return;
            }
        }
    }
    close.cancel();
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::Sink;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    #[derive(Debug)]
    struct FailingSink;

    impl Sink<Bytes> for FailingSink {
        type Error = std::io::Error;

        fn poll_ready(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn start_send(self: Pin<&mut Self>, _item: Bytes) -> Result<(), Self::Error> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "fixture writer failure",
            ))
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn writer_failure_cancels_its_connection() {
        let (sender, mut receiver) = mpsc::channel(1);
        sender
            .send(Outbound::Frames(Arc::from(vec![
                WireFrame::from_text("#TMserver:ECP1:test").unwrap(),
            ])))
            .await
            .unwrap();
        drop(sender);
        let close = CancellationToken::new();
        let mut sink = FailingSink;

        run_writer(ConnectionId(1), &mut sink, &mut receiver, &close).await;
        assert!(close.is_cancelled());
    }
}
