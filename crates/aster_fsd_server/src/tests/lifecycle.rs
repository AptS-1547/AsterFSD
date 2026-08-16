use super::support::*;
use aster_fsd_model::ProtocolDialect;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::{Duration, timeout};

#[tokio::test]
async fn idle_timeout_closes_only_the_idle_connection() {
    let mut config = listener("classic", ProtocolDialect::Classic, 511);
    config.idle_timeout_seconds = 1;
    let (addresses, shutdown, task) = start_server(vec![config]).await;
    let mut stream = TcpStream::connect(addresses[0]).await.unwrap();
    assert_connection_closes(&mut stream).await;
    shutdown.cancel();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn oversized_unterminated_frame_is_closed_at_codec_boundary() {
    let (addresses, shutdown, task) = start_server(vec![listener(
        "classic-boundary",
        ProtocolDialect::Classic,
        8,
    )])
    .await;
    let mut stream = TcpStream::connect(addresses[0]).await.unwrap();
    stream.write_all(b"123456789").await.unwrap();
    assert_connection_closes(&mut stream).await;
    shutdown.cancel();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn reset_and_logoff_release_callsign_and_emit_one_exact_removal() {
    let (address, shutdown, task) = start_classic_server().await;
    let observer = TcpStream::connect(address).await.unwrap();
    let (observer_read, mut observer_write) = observer.into_split();
    let mut observer_read = BufReader::new(observer_read);
    observer_write
        .write_all(b"#AAATC1:SERVER:Observer:CID1:secret:5:9\r\n")
        .await
        .unwrap();
    assert!(
        read_line(&mut observer_read)
            .await
            .starts_with("#TMserver:ATC1:")
    );

    let reset_client = TcpStream::connect(address).await.unwrap();
    socket2::SockRef::from(&reset_client)
        .set_linger(Some(Duration::ZERO))
        .unwrap();
    let (reset_read, mut reset_write) = reset_client.into_split();
    let mut reset_read = BufReader::new(reset_read);
    reset_write
        .write_all(b"#APPILOT1:SERVER:CID2:secret:1:9:2:Reset Pilot\r\n")
        .await
        .unwrap();
    assert!(
        read_line(&mut reset_read)
            .await
            .starts_with("#TMserver:PILOT1:")
    );
    assert_eq!(
        read_line(&mut observer_read).await,
        "#APPILOT1:SERVER:CID2::1:9:2\r\n"
    );
    drop(reset_read);
    drop(reset_write);
    assert_eq!(read_line(&mut observer_read).await, "#DPPILOT1:CID2\r\n");
    assert_no_bytes(&mut observer_read).await;

    let logoff_client = TcpStream::connect(address).await.unwrap();
    let (logoff_read, mut logoff_write) = logoff_client.into_split();
    let mut logoff_read = BufReader::new(logoff_read);
    logoff_write
        .write_all(b"#APPILOT1:SERVER:CID2:secret:1:9:2:Logoff Pilot\r\n")
        .await
        .unwrap();
    assert!(
        read_line(&mut logoff_read)
            .await
            .starts_with("#TMserver:PILOT1:")
    );
    assert_eq!(
        read_line(&mut observer_read).await,
        "#APPILOT1:SERVER:CID2::1:9:2\r\n"
    );
    logoff_write.write_all(b"#DPPILOT1:CID2\r\n").await.unwrap();
    assert_eq!(read_line(&mut observer_read).await, "#DPPILOT1:CID2\r\n");
    assert_reader_closes(&mut logoff_read).await;
    assert_no_bytes(&mut observer_read).await;

    shutdown.cancel();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn graceful_shutdown_closes_connected_clients_and_listener_tasks() {
    let (address, shutdown, task) = start_classic_server().await;
    let mut stream = TcpStream::connect(address).await.unwrap();
    shutdown.cancel();
    assert_connection_closes(&mut stream).await;
    timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
