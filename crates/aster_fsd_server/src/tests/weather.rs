use super::support::*;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[tokio::test]
async fn classic_request_returns_c_exact_three_frame_profile() {
    let (address, shutdown, task) = start_classic_server().await;
    let stream = TcpStream::connect(address).await.unwrap();
    let (read, mut write) = stream.into_split();
    let mut read = BufReader::new(read);
    write
        .write_all(b"#APECP1:SERVER:CID1:secret:1:9:2:Weather Pilot\r\n")
        .await
        .unwrap();
    assert!(read_line(&mut read).await.starts_with("#TMserver:ECP1:"));

    write.write_all(b"#WXECP1:SERVER:KJFK\r\n").await.unwrap();
    assert_eq!(
        read_line(&mut read).await,
        "#TDserver:ECP1:100:15:100:15:100:15:100:15:2992\r\n"
    );
    assert_eq!(
        read_line(&mut read).await,
        "#WDserver:ECP1:2500:0:180:12:0:1:2500:0:180:12:0:1:2500:0:180:12:0:1:2500:0:180:12:0:1\r\n"
    );
    assert_eq!(
        read_line(&mut read).await,
        "#CDserver:ECP1:5000:3000:4:0:1:5000:3000:4:0:1:35000:20000:1:2:3:12.50\r\n"
    );

    write
        .write_all(b"$AXECP1:SERVER:METAR:KJFK\r\n")
        .await
        .unwrap();
    assert_eq!(
        read_line(&mut read).await,
        "$ARserver:ECP1:METAR:KJFK 161651Z 18012KT 10SM FEW030 15/08 A2992\r\n"
    );

    write.write_all(b"#WXECP1:SERVER:ZZZZ\r\n").await.unwrap();
    assert_eq!(
        read_line(&mut read).await,
        "$ERserver:ECP1:009:ZZZZ:No such weather profile\r\n"
    );

    shutdown.cancel();
    task.await.unwrap().unwrap();
}
