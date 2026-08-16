//! Minimal client-speaks-first classic FSD session example.
//!
//! Run with `cargo run --example simple_client` while a local server listens on
//! `127.0.0.1:6809`.

#![deny(
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::unreachable,
    clippy::unwrap_used
)]

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

// Example FSD protocol values
const EXAMPLE_CALLSIGN: &str = "TEST123";
const EXAMPLE_CID: &str = "1234567"; // Example VATSIM CID
const EXAMPLE_PASSWORD: &str = "password"; // Placeholder - not a real password

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("AsterFSD Classic Client Example");
    println!("=========================\n");

    // Connect to the FSD server
    let server_addr = "127.0.0.1:6809";
    println!("Connecting to {server_addr}...");

    let stream = TcpStream::connect(server_addr).await?;
    println!("Connected!\n");

    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Spawn a task to read responses from server
    let read_handle = tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    println!("Server closed connection");
                    break;
                }
                Ok(_) => {
                    print!("< {line}");
                }
                Err(e) => {
                    eprintln!("Error reading from server: {e}");
                    break;
                }
            }
        }
    });

    // Classic FSD is client-speaks-first and uses draft revision 9.
    let login_packet = format!(
        "#AP{EXAMPLE_CALLSIGN}:SERVER:{EXAMPLE_CID}:{EXAMPLE_PASSWORD}:1:9:2:John Doe KJFK\r\n"
    );
    println!("> {}", login_packet.trim_end());
    writer.write_all(login_packet.as_bytes()).await?;
    writer.flush().await?;

    // Wait a bit
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Send a position update
    let pos_packet =
        format!("@N:{EXAMPLE_CALLSIGN}:1200:1:40.6413:-73.7781:5000:250:414141414:30\r\n");
    println!("> {}", pos_packet.trim_end());
    writer.write_all(pos_packet.as_bytes()).await?;
    writer.flush().await?;

    // Wait a bit
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Send a text message
    let msg_packet = format!("#TM{EXAMPLE_CALLSIGN}:*:Hello from the example client!\r\n");
    println!("> {}", msg_packet.trim_end());
    writer.write_all(msg_packet.as_bytes()).await?;
    writer.flush().await?;

    // Wait a bit
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Send logoff
    let logoff_packet = format!("#DP{EXAMPLE_CALLSIGN}:{EXAMPLE_CID}\r\n");
    println!("> {}", logoff_packet.trim_end());
    writer.write_all(logoff_packet.as_bytes()).await?;
    writer.flush().await?;

    println!("\nClosing connection...");
    drop(writer);

    // Wait for reader to finish
    let _ = tokio::time::timeout(tokio::time::Duration::from_secs(2), read_handle).await;

    println!("Disconnected.");
    Ok(())
}
