//! Interactive classic FSD test client.
//!
//! Run with `cargo run --example test_client` while a local server listens on
//! `127.0.0.1:6809`.

#![deny(
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::unreachable,
    clippy::unwrap_used
)]

use std::io::{self, Write};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

const DEFAULT_CALLSIGN: &str = "TEST123";
const DEFAULT_CID: &str = "1234567";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔════════════════════════════════════════╗");
    println!("║   AsterFSD Interactive Test Client    ║");
    println!("╚════════════════════════════════════════╝\n");

    // Connect to the FSD server
    let server_addr = "127.0.0.1:6809";
    println!("🔌 Connecting to {server_addr}...");

    let stream = TcpStream::connect(server_addr).await?;
    println!("✅ Connected!\n");

    let (reader, mut writer) = stream.into_split();
    spawn_reader(BufReader::new(reader));
    run_command_loop(&mut writer).await?;
    drop(writer);
    println!("✅ Disconnected.");
    Ok(())
}

fn spawn_reader(mut reader: BufReader<tokio::net::tcp::OwnedReadHalf>) {
    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    println!("\n⚠️  Server closed connection");
                    break;
                }
                Ok(_) => {
                    print!("📥 {line}");
                    let _ = io::stdout().flush();
                }
                Err(e) => {
                    eprintln!("\n❌ Error reading from server: {e}");
                    break;
                }
            }
        }
    });
}

async fn run_command_loop(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut callsign = DEFAULT_CALLSIGN.to_string();
    let mut logged_in = false;
    print_help();
    loop {
        print!("\n> ");
        io::stdout().flush()?;
        let input = tokio::task::spawn_blocking(|| {
            let mut buffer = String::new();
            io::stdin().read_line(&mut buffer).ok().map(|_| buffer)
        })
        .await;
        let Ok(Some(input)) = input else {
            continue;
        };
        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        if handle_command(writer, &mut callsign, &mut logged_in, input).await? {
            break;
        }
    }
    Ok(())
}

async fn handle_command(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &mut String,
    logged_in: &mut bool,
    input: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    match input.split_whitespace().next().unwrap_or("") {
        "help" | "h" => {
            print_help();
        }
        "quit" | "q" | "exit" => {
            println!("👋 Disconnecting...");
            if *logged_in {
                let logoff = format!("#DP{callsign}:{DEFAULT_CID}\r\n");
                let _ = writer.write_all(logoff.as_bytes()).await;
                let _ = writer.flush().await;
            }
            return Ok(true);
        }
        "id" => {
            let parts: Vec<&str> = input.split_whitespace().collect();
            if parts.len() > 1 {
                *callsign = parts[1].to_string();
            }
            send_identification(writer, callsign).await?;
        }
        "login" => {
            let parts: Vec<&str> = input.split_whitespace().collect();
            let client_type = parts.get(1).unwrap_or(&"pilot");
            send_login(writer, callsign, client_type).await?;
            *logged_in = true;
        }
        "logoff" => {
            send_logoff(writer, callsign).await?;
            *logged_in = false;
        }
        "pos" => {
            let parts: Vec<&str> = input.split_whitespace().collect();
            let lat = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(40.6413);
            let lon = parts
                .get(2)
                .and_then(|s| s.parse().ok())
                .unwrap_or(-73.7781);
            let alt = parts.get(3).and_then(|s| s.parse().ok()).unwrap_or(5000);
            send_position(writer, callsign, lat, lon, alt).await?;
        }
        "msg" => {
            let parts: Vec<&str> = input.splitn(3, ' ').collect();
            let to = parts.get(1).unwrap_or(&"*");
            let message = parts.get(2).unwrap_or(&"Test message");
            send_message(writer, callsign, to, message).await?;
        }
        "fp" => {
            send_flight_plan(writer, callsign).await?;
        }
        "metar" => {
            let parts: Vec<&str> = input.split_whitespace().collect();
            let icao = parts.get(1).unwrap_or(&"KJFK");
            send_metar_request(writer, callsign, icao).await?;
        }
        "caps" => {
            send_caps_response(writer, callsign).await?;
        }
        "rn" => {
            let parts: Vec<&str> = input.split_whitespace().collect();
            let target = parts.get(1).unwrap_or(&"*");
            send_realname_request(writer, callsign, target).await?;
        }
        "raw" => {
            let raw_packet = input.strip_prefix("raw ").unwrap_or("");
            if !raw_packet.is_empty() {
                let packet = format!("{raw_packet}\r\n");
                println!("📤 {}", packet.trim_end());
                writer.write_all(packet.as_bytes()).await?;
                writer.flush().await?;
            }
        }
        "test" => {
            println!("🧪 Running automated test sequence...\n");
            run_test_sequence(writer, callsign).await?;
            *logged_in = true;
        }
        _ => {
            println!("❓ Unknown command. Type 'help' for available commands.");
        }
    }
    Ok(false)
}

fn print_help() {
    println!("\n📖 Available Commands:");
    println!("  help, h              - Show this help");
    println!("  id [callsign]        - Send identification (default: TEST123)");
    println!("  login [pilot|atc]    - Login as pilot or ATC (default: pilot)");
    println!("  logoff               - Send logoff");
    println!("  pos [lat] [lon] [alt]- Send position update (default: JFK)");
    println!("  msg [to] [text]      - Send text message (default: broadcast)");
    println!("  fp                   - Send sample flight plan");
    println!("  metar [icao]         - Request METAR (default: KJFK)");
    println!("  caps                 - Send capabilities response");
    println!("  rn [callsign]        - Request real name");
    println!("  raw [packet]         - Send raw FSD packet");
    println!("  test                 - Run automated test sequence");
    println!("  quit, q, exit        - Disconnect and exit");
}

async fn send_identification(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet =
        format!("$ID{callsign}:SERVER:69d7:AsterFSD Test Client:3:2:{DEFAULT_CID}:987654321\r\n");
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_login(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
    client_type: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = match client_type {
        "atc" | "ATC" => {
            // #AA(callsign):SERVER:(full name):(network ID):(password):(rating):(protocol version)
            format!("#AA{callsign}:SERVER:Test Controller:{DEFAULT_CID}:password:5:9\r\n")
        }
        _ => {
            // #AP(callsign):SERVER:(network ID):(password):(rating):(protocol version):(num2):(full name ICAO)
            format!("#AP{callsign}:SERVER:{DEFAULT_CID}:password:1:9:2:Test Pilot KJFK\r\n")
        }
    };
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_logoff(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = format!("#DP{callsign}:{DEFAULT_CID}\r\n");
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_position(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
    lat: f64,
    lon: f64,
    alt: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = format!("@N:{callsign}:1200:1:{lat}:{lon}:{alt}:250:414141414:30\r\n");
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_message(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
    to: &str,
    message: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = format!("#TM{callsign}:{to}:{message}\r\n");
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_flight_plan(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = format!(
        "$FP{callsign}:*A:V:B738:420:KJFK:1200:1200:35000:KLAX:03:30:02:45:KSFO:Remarks here:DCT ROUTE\r\n"
    );
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_metar_request(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
    icao: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = format!("$AX{callsign}:SERVER:METAR:{icao}\r\n");
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_caps_response(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = format!("$CR{callsign}:SERVER:CAPS:ATCINFO=1:MODELDESC=1:ACCONFIG=1\r\n");
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_realname_request(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
    target: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = format!("$CQ{callsign}:{target}:RN\r\n");
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn run_test_sequence(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("1️⃣  Sending identification...");
    send_identification(writer, callsign).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\n2️⃣  Logging in as pilot...");
    send_login(writer, callsign, "pilot").await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\n3️⃣  Sending position update...");
    send_position(writer, callsign, 40.6413, -73.7781, 5000).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\n4️⃣  Sending broadcast message...");
    send_message(writer, callsign, "*", "Hello from test client!").await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\n5️⃣  Filing flight plan...");
    send_flight_plan(writer, callsign).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\n6️⃣  Requesting METAR...");
    send_metar_request(writer, callsign, "KJFK").await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\n✅ Test sequence completed!");
    Ok(())
}
