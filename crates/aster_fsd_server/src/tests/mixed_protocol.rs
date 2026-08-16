use super::support::*;
use aster_fsd_model::ProtocolDialect;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[tokio::test]
async fn classic_and_aster_clients_share_one_authoritative_network() {
    let (addresses, shutdown, task) = start_server(vec![
        listener("classic", ProtocolDialect::Classic, 511),
        listener("aster", ProtocolDialect::AsterV1, 16_384),
    ])
    .await;

    let classic = TcpStream::connect(addresses[0]).await.unwrap();
    let (classic_read, mut classic_write) = classic.into_split();
    let mut classic_read = BufReader::new(classic_read);
    classic_write
        .write_all(b"#APECP1:SERVER:CID1:classic-secret:1:9:2:Classic Pilot\r\n")
        .await
        .unwrap();
    assert!(
        read_line(&mut classic_read)
            .await
            .starts_with("#TMserver:ECP1:")
    );

    let aster = TcpStream::connect(addresses[1]).await.unwrap();
    let (aster_read, mut aster_write) = aster.into_split();
    let mut aster_read = BufReader::new(aster_read);
    let hello: serde_json::Value =
        serde_json::from_str(read_line(&mut aster_read).await.trim()).unwrap();
    assert_eq!(hello["type"], "hello");
    aster_write
        .write_all(
            br#"{"v":1,"type":"login","callsign":"ECP2","client_type":"pilot","network_id":"CID2","password":"aster-secret","requested_rating":1,"real_name":"Aster Pilot","simulator_type":2}"#,
        )
        .await
        .unwrap();
    aster_write.write_all(b"\r\n").await.unwrap();
    let welcome: serde_json::Value =
        serde_json::from_str(read_line(&mut aster_read).await.trim()).unwrap();
    assert_eq!(welcome["type"], "welcome");

    let presence = read_line(&mut classic_read).await;
    assert_eq!(presence, "#APECP2:SERVER:CID2::1:9:2\r\n");
    assert!(!presence.contains("aster-secret"));

    aster_write
        .write_all(
            br#"{"v":1,"type":"text","source":"ECP2","destination":{"kind":"direct","value":"ECP1"},"message":"cross protocol"}"#,
        )
        .await
        .unwrap();
    aster_write.write_all(b"\r\n").await.unwrap();
    assert_eq!(
        read_line(&mut classic_read).await,
        "#TMECP2:ECP1:cross protocol\r\n"
    );

    shutdown.cancel();
    task.await.unwrap().unwrap();
}
