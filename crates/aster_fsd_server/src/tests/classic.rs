use super::support::*;
use aster_fsd_model::ProtocolDialect;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[tokio::test]
async fn login_error_wire_and_close_policy_match_c() {
    let (address, shutdown, task) = start_classic_server().await;

    for (frame, expected) in [
        (
            b"#APX:SERVER:CID1:secret:1:9:2:Invalid Callsign\r\n".as_slice(),
            "$ERserver:unknown:002::Invalid callsign\r\n",
        ),
        (
            b"#APECP1:SERVER:CID1:secret:1:8:2:Wrong Revision\r\n".as_slice(),
            "$ERserver:ECP1:010::Invalid protocol revision\r\n",
        ),
        (
            b"#APECP1:SERVER:CID1:secret:1:invalid:2:Bad Revision\r\n".as_slice(),
            "$ERserver:unknown:010::Invalid protocol revision\r\n",
        ),
    ] {
        let stream = TcpStream::connect(address).await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut read = BufReader::new(read);
        write.write_all(frame).await.unwrap();
        assert_eq!(read_line(&mut read).await, expected);
        assert_reader_closes(&mut read).await;
    }

    let stream = TcpStream::connect(address).await.unwrap();
    let (read, mut write) = stream.into_split();
    let mut read = BufReader::new(read);
    write
        .write_all(b"#APECP1:SERVER:CID1:secret\r\n")
        .await
        .unwrap();
    assert_eq!(
        read_line(&mut read).await,
        "$ERserver:unknown:004::Syntax error\r\n"
    );
    write
        .write_all(b"#APECP1:SERVER:CID1:secret:1:9:2:Valid Pilot\r\n")
        .await
        .unwrap();
    assert!(read_line(&mut read).await.starts_with("#TMserver:ECP1:"));

    write
        .write_all(b"#APECP1:SERVER:CID1:secret:1:9:2:Repeated Pilot\r\n")
        .await
        .unwrap();
    assert_eq!(
        read_line(&mut read).await,
        "$ERserver:ECP1:003::Already registerd\r\n"
    );

    let peer = TcpStream::connect(address).await.unwrap();
    let (peer_read, mut peer_write) = peer.into_split();
    let mut peer_read = BufReader::new(peer_read);
    peer_write
        .write_all(b"#APPEER1:SERVER:CID2:secret:1:9:2:Peer Pilot\r\n")
        .await
        .unwrap();
    assert!(
        read_line(&mut peer_read)
            .await
            .starts_with("#TMserver:PEER1:")
    );
    assert_eq!(
        read_line(&mut read).await,
        "#APPEER1:SERVER:CID2::1:9:2\r\n"
    );

    shutdown.cancel();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn credential_and_suspension_failures_are_private_and_close() {
    for (failure, expected) in [
        (
            LoginFailure::InvalidCredentials,
            "$ERserver:ECP1:006:CID1:Invalid CID/password\r\n",
        ),
        (
            LoginFailure::Suspended,
            "$ERserver:ECP1:013::CID/PID was suspended\r\n",
        ),
    ] {
        let (addresses, shutdown, task) = start_server_with_failure(
            vec![listener("classic", ProtocolDialect::Classic, 511)],
            failure,
        )
        .await;
        let stream = TcpStream::connect(addresses[0]).await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut read = BufReader::new(read);
        write
            .write_all(b"#APECP1:SERVER:CID1:never-log-this:1:9:2:Rejected Pilot\r\n")
            .await
            .unwrap();
        assert_eq!(read_line(&mut read).await, expected);
        assert_reader_closes(&mut read).await;
        shutdown.cancel();
        task.await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn multi_client_login_is_targeted_and_presence_is_sanitized() {
    let (address, shutdown, task) = start_classic_server().await;
    let first = TcpStream::connect(address).await.unwrap();
    let (first_read, mut first_write) = first.into_split();
    let mut first_read = BufReader::new(first_read);
    assert_no_bytes(&mut first_read).await;
    first_write
        .write_all(b"#APECP1:SERVER:CID1:first-secret:1:9:2:First Pilot\r\n")
        .await
        .unwrap();
    assert!(
        read_line(&mut first_read)
            .await
            .starts_with("#TMserver:ECP1:")
    );

    let second = TcpStream::connect(address).await.unwrap();
    let (second_read, mut second_write) = second.into_split();
    let mut second_read = BufReader::new(second_read);
    second_write
        .write_all(b"#APECP2:SERVER:CID2:second-secret:1:9:2:Second Pilot\r\n")
        .await
        .unwrap();
    let second_welcome = read_line(&mut second_read).await;
    assert!(second_welcome.starts_with("#TMserver:ECP2:"));
    let first_presence = read_line(&mut first_read).await;
    assert_eq!(first_presence, "#APECP2:SERVER:CID2::1:9:2\r\n");
    assert!(!first_presence.contains("second-secret"));

    first_write
        .write_all(b"#TMECP1:ECP2:private message\r\n")
        .await
        .unwrap();
    assert_eq!(
        read_line(&mut second_read).await,
        "#TMECP1:ECP2:private message\r\n"
    );
    assert_no_bytes(&mut first_read).await;

    second_write
        .write_all(b"@NECP2:1200:1:31.23000:121.47000:5000:200:0:0\r\n")
        .await
        .unwrap();
    assert_no_bytes(&mut first_read).await;
    first_write
        .write_all(b"@NECP1:0000:1:31.23100:121.47000:5000:200:0:0\r\n")
        .await
        .unwrap();
    assert_eq!(
        read_line(&mut second_read).await,
        "@N:ECP1:0:1:31.23100:121.47000:5000:200:0:0\r\n"
    );

    shutdown.cancel();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn typed_direct_commands_match_c_relay_and_error_semantics() {
    let (address, shutdown, task) = start_classic_server().await;
    let first = TcpStream::connect(address).await.unwrap();
    let (first_read, mut first_write) = first.into_split();
    let mut first_read = BufReader::new(first_read);
    first_write
        .write_all(b"#APECP1:SERVER:CID1:first-secret:1:9:2:First Pilot\r\n")
        .await
        .unwrap();
    assert!(
        read_line(&mut first_read)
            .await
            .starts_with("#TMserver:ECP1:")
    );

    let second = TcpStream::connect(address).await.unwrap();
    let (second_read, mut second_write) = second.into_split();
    let mut second_read = BufReader::new(second_read);
    second_write
        .write_all(b"#APECP2:SERVER:CID2:second-secret:1:9:2:Second Pilot\r\n")
        .await
        .unwrap();
    assert!(
        read_line(&mut second_read)
            .await
            .starts_with("#TMserver:ECP2:")
    );
    assert_eq!(
        read_line(&mut first_read).await,
        "#APECP2:SERVER:CID2::1:9:2\r\n"
    );

    for frame in [
        b"$HOECP1:ECP2:ECP3:123.450\r\n".as_slice(),
        b"#SBECP1:ECP2\r\n",
        b"#PCECP1:ECP2:VERSION\r\n",
        b"$C?ECP1:ECP2\r\n",
    ] {
        first_write.write_all(frame).await.unwrap();
        assert_eq!(read_line(&mut second_read).await.as_bytes(), frame);
        assert_no_bytes(&mut first_read).await;
    }

    for frame in [
        b"$HAECP2:ECP1:ECP3\r\n".as_slice(),
        b"$CIECP2:ECP1:123.450\r\n",
    ] {
        second_write.write_all(frame).await.unwrap();
        assert_eq!(read_line(&mut first_read).await.as_bytes(), frame);
        assert_no_bytes(&mut second_read).await;
    }

    first_write.write_all(b"#SBECP1:MISSING\r\n").await.unwrap();
    assert_no_bytes(&mut first_read).await;
    assert_no_bytes(&mut second_read).await;

    first_write.write_all(b"$HOECP1:ECP2\r\n").await.unwrap();
    assert_eq!(
        read_line(&mut first_read).await,
        "$ERserver:ECP1:004::Syntax error\r\n"
    );
    assert_no_bytes(&mut second_read).await;

    first_write
        .write_all(b"$HOFAKE:ECP2:ECP3\r\n")
        .await
        .unwrap();
    assert_eq!(
        read_line(&mut first_read).await,
        "$ERserver:ECP1:005:FAKE:Invalid source callsign\r\n"
    );
    assert_no_bytes(&mut second_read).await;

    shutdown.cancel();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn queries_match_c_direct_relay_and_flight_plan_response() {
    let (address, shutdown, task) = start_classic_server().await;
    let first = TcpStream::connect(address).await.unwrap();
    let (first_read, mut first_write) = first.into_split();
    let mut first_read = BufReader::new(first_read);
    first_write
        .write_all(b"#APECP1:SERVER:CID1:first-secret:1:9:2:First Pilot\r\n")
        .await
        .unwrap();
    assert!(
        read_line(&mut first_read)
            .await
            .starts_with("#TMserver:ECP1:")
    );

    let second = TcpStream::connect(address).await.unwrap();
    let (second_read, mut second_write) = second.into_split();
    let mut second_read = BufReader::new(second_read);
    second_write
        .write_all(b"#APECP2:SERVER:CID2:second-secret:1:9:2:Second Pilot\r\n")
        .await
        .unwrap();
    assert!(
        read_line(&mut second_read)
            .await
            .starts_with("#TMserver:ECP2:")
    );
    assert_eq!(
        read_line(&mut first_read).await,
        "#APECP2:SERVER:CID2::1:9:2\r\n"
    );

    for frame in [
        b"$CQECP1:ECP2:CAPS\r\n".as_slice(),
        b"$CQECP1:ECP2:ACC:CONFIG:FULL\r\n",
        b"$CQECP1:ECP2:ATIS\r\n",
        b"$CQECP1:ECP2:INF\r\n",
    ] {
        first_write.write_all(frame).await.unwrap();
        assert_eq!(read_line(&mut second_read).await.as_bytes(), frame);
        assert_no_bytes(&mut first_read).await;
    }

    for frame in [
        b"$CRECP2:ECP1:CAPS:VERSION=1\r\n".as_slice(),
        b"$CRECP2:ECP1:ACC:CONFIG=FULL\r\n",
        b"$CRECP2:ECP1:ATIS:LINE=Test ATIS\r\n",
        b"$CRECP2:ECP1:INF:CLIENT=swift\r\n",
    ] {
        second_write.write_all(frame).await.unwrap();
        assert_eq!(read_line(&mut first_read).await.as_bytes(), frame);
        assert_no_bytes(&mut second_read).await;
    }

    second_write
        .write_all(b"$CRECP2:ECP1:CAPS\r\n")
        .await
        .unwrap();
    assert_eq!(
        read_line(&mut second_read).await,
        "$ERserver:ECP2:004::Syntax error\r\n"
    );
    assert_no_bytes(&mut first_read).await;

    first_write
        .write_all(b"$CQECP1:SERVER:CAPS\r\n")
        .await
        .unwrap();
    assert_no_bytes(&mut first_read).await;
    assert_no_bytes(&mut second_read).await;

    let flight_plan =
        b"$FPECP1:*A:I:B738:450:ZSPD:1200:1205:FL350:ZBAA:2:0:4:0:ZSNJ:RMK:DCT PIKAS DCT\r\n";
    first_write.write_all(flight_plan).await.unwrap();
    assert_no_bytes(&mut first_read).await;
    assert_no_bytes(&mut second_read).await;

    second_write
        .write_all(b"$CQECP2:SERVER:FP:ECP1\r\n")
        .await
        .unwrap();
    assert_eq!(
        read_line(&mut second_read).await,
        "$FPECP1:ECP2:I:B738:450:ZSPD:1200:1205:FL350:ZBAA:2:0:4:0:ZSNJ:RMK:DCT PIKAS DCT\r\n"
    );
    assert_no_bytes(&mut first_read).await;

    shutdown.cancel();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn kill_matches_c_authority_notice_wire_and_close_order() {
    let (address, shutdown, task) = start_classic_server().await;
    let supervisor = TcpStream::connect(address).await.unwrap();
    let (supervisor_read, mut supervisor_write) = supervisor.into_split();
    let mut supervisor_read = BufReader::new(supervisor_read);
    supervisor_write
        .write_all(b"#AAECP1:SERVER:Supervisor:CID1:admin-secret:11:9\r\n")
        .await
        .unwrap();
    assert!(
        read_line(&mut supervisor_read)
            .await
            .starts_with("#TMserver:ECP1:")
    );

    let pilot = TcpStream::connect(address).await.unwrap();
    let (pilot_read, mut pilot_write) = pilot.into_split();
    let mut pilot_read = BufReader::new(pilot_read);
    pilot_write
        .write_all(b"#APECP2:SERVER:CID2:pilot-secret:1:9:2:Second Pilot\r\n")
        .await
        .unwrap();
    assert!(
        read_line(&mut pilot_read)
            .await
            .starts_with("#TMserver:ECP2:")
    );
    assert_eq!(
        read_line(&mut supervisor_read).await,
        "#APECP2:SERVER:CID2::1:9:2\r\n"
    );

    pilot_write
        .write_all(b"$!!ECP2:MISSING:reason\r\n")
        .await
        .unwrap();
    assert_eq!(
        read_line(&mut pilot_read).await,
        "$ERserver:ECP2:007:MISSING:No such callsign\r\n"
    );

    pilot_write
        .write_all(b"$!!ECP2:ECP1:reason\r\n")
        .await
        .unwrap();
    assert_eq!(
        read_line(&mut pilot_read).await,
        "#TMserver:ECP2:You are not allowed to kill users!\r\n"
    );

    supervisor_write
        .write_all(b"$!!ECP1:ECP2:network abuse\r\n")
        .await
        .unwrap();
    assert_eq!(
        read_line(&mut supervisor_read).await,
        "#TMserver:ECP1:Attempting to kill ECP2\r\n"
    );
    assert_eq!(
        read_line(&mut pilot_read).await,
        "$!!SERVER:ECP2:network abuse\r\n"
    );
    assert_eq!(read_line(&mut supervisor_read).await, "#DPECP2:CID2\r\n");
    assert_reader_closes(&mut pilot_read).await;

    shutdown.cancel();
    task.await.unwrap().unwrap();
}
