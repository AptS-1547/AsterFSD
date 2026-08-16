use super::support::*;
use aster_fsd_model::ProtocolDialect;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[tokio::test]
async fn listener_owns_handshake_login_profile_and_revision_100_presence() {
    let (addresses, shutdown, task) =
        start_server(vec![listener("vatsim", ProtocolDialect::Vatsim, 4096)]).await;
    let stream = TcpStream::connect(addresses[0]).await.unwrap();
    let (read, mut write) = stream.into_split();
    let mut read = BufReader::new(read);
    assert!(
        read_line(&mut read)
            .await
            .starts_with("$DISERVER:CLIENT:VATSIM FSD V3.13:")
    );
    write
        .write_all(b"$IDECP3:SERVER:48e2:swift:3:2:CID3:987654321\r\n")
        .await
        .unwrap();
    write
        .write_all(b"#APECP3:SERVER:CID3:vatsim-secret:1:100:2:VATSIM Pilot\r\n")
        .await
        .unwrap();
    assert_eq!(
        read_line(&mut read).await,
        "#TMserver:ECP3:AsterFSD 0.2.0\r\n"
    );
    assert_eq!(read_line(&mut read).await, "$CQSERVER:ECP3:CAPS\r\n");
    assert_eq!(
        read_line(&mut read).await,
        "$CRSERVER:ECP3:IP:127.0.0.1\r\n"
    );
    assert_eq!(
        read_line(&mut read).await,
        "$ERserver:ECP3:008:ECP3:No flightplan\r\n"
    );

    let atc = TcpStream::connect(addresses[0]).await.unwrap();
    let (atc_read, mut atc_write) = atc.into_split();
    let mut atc_read = BufReader::new(atc_read);
    assert!(
        read_line(&mut atc_read)
            .await
            .starts_with("$DISERVER:CLIENT:VATSIM FSD V3.13:")
    );
    atc_write
        .write_all(b"$IDZSPD_TWR:SERVER:48e2:swift:3:2:CID4:-987654321\r\n")
        .await
        .unwrap();
    atc_write
        .write_all(b"#AAZSPD_TWR:SERVER:VATSIM ATC:CID4:vatsim-secret:5:100\r\n")
        .await
        .unwrap();

    assert_eq!(
        read_line(&mut read).await,
        "#AAZSPD_TWR:SERVER:CID4:CID4::5:100\r\n"
    );
    assert_eq!(
        read_line(&mut atc_read).await,
        "#TMserver:ZSPD_TWR:AsterFSD 0.2.0\r\n"
    );
    assert_eq!(
        read_line(&mut atc_read).await,
        "$CQSERVER:ZSPD_TWR:CAPS\r\n"
    );
    assert_eq!(
        read_line(&mut atc_read).await,
        "$CRSERVER:ZSPD_TWR:ATC:N:ZSPD_TWR\r\n"
    );
    assert_eq!(
        read_line(&mut atc_read).await,
        "$CRSERVER:ZSPD_TWR:CAPS:ATCINFO=1:SECPOS=1\r\n"
    );
    assert_eq!(
        read_line(&mut atc_read).await,
        "$CRSERVER:ZSPD_TWR:IP:127.0.0.1\r\n"
    );

    let second_pilot = TcpStream::connect(addresses[0]).await.unwrap();
    let (second_pilot_read, mut second_pilot_write) = second_pilot.into_split();
    let mut second_pilot_read = BufReader::new(second_pilot_read);
    assert!(
        read_line(&mut second_pilot_read)
            .await
            .starts_with("$DISERVER:CLIENT:VATSIM FSD V3.13:")
    );
    second_pilot_write
        .write_all(b"$IDPILOT2:SERVER:48e2:swift:3:2:CID5:+987654321\r\n")
        .await
        .unwrap();
    second_pilot_write
        .write_all(b"#APPILOT2:SERVER:CID5:vatsim-secret:1:100:2:VATSIM Pilot\r\n")
        .await
        .unwrap();
    assert_eq!(
        read_line(&mut atc_read).await,
        "#APPILOT2:SERVER:CID5::1:100:2:CID5\r\n"
    );

    shutdown.cancel();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn revision_and_identification_ownership_fail_with_exact_wire_and_close() {
    let (addresses, shutdown, task) =
        start_server(vec![listener("vatsim", ProtocolDialect::Vatsim, 4096)]).await;

    for (identification, login, expected) in [
        (
            b"$IDECP5:SERVER:48e2:swift:3:2:CID5:987654321\r\n".as_slice(),
            b"#APECP5:SERVER:CID5:secret:1:9:2:Wrong Revision\r\n".as_slice(),
            "$ERserver:ECP5:010::Invalid protocol revision\r\n",
        ),
        (
            b"$IDECP6:SERVER:48e2:swift:3:2:CID6:987654321\r\n".as_slice(),
            b"#APECP6:SERVER:OTHER:secret:1:100:2:Wrong CID\r\n".as_slice(),
            "$ERserver:ECP6:005::Invalid source callsign\r\n",
        ),
    ] {
        let stream = TcpStream::connect(addresses[0]).await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut read = BufReader::new(read);
        assert!(
            read_line(&mut read)
                .await
                .starts_with("$DISERVER:CLIENT:VATSIM FSD V3.13:")
        );
        write.write_all(identification).await.unwrap();
        write.write_all(login).await.unwrap();
        assert_eq!(read_line(&mut read).await, expected);
        assert_reader_closes(&mut read).await;
    }

    shutdown.cancel();
    task.await.unwrap().unwrap();
}
