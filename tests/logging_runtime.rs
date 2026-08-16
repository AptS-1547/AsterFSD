#![cfg(unix)]

use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static RUN_ID: AtomicU64 = AtomicU64::new(1);

struct RunOptions<'a> {
    configured_level: &'a str,
    rust_log: Option<&'a str>,
    sqlx_logging: bool,
    sqlx_logging_level: &'a str,
}

struct RunOutput {
    logs: String,
    response: Vec<u8>,
}

fn reserve_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("ephemeral port must be available")
        .local_addr()
        .expect("ephemeral listener must have a local address")
        .port()
}

fn temp_directory() -> PathBuf {
    let run_id = RUN_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "asterfsd-logging-test-{}-{run_id}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("logging test directory must be creatable");
    path
}

fn write_config(directory: &Path, port: u16, options: &RunOptions<'_>) {
    let config = format!(
        r#"[server]
name = "AsterFSD Logging Test"
version = "0.2.0"
max_clients = 16
mailbox_capacity = 8
wind_delta_interval_seconds = 70
motd = []

[[listeners]]
name = "classic"
protocol = "classic"
address = "127.0.0.1"
port = {port}
max_frame_bytes = 511
idle_timeout_seconds = 5

[logging]
level = "{}"
format = "text"
file = ""
enable_rotation = false
max_backups = 1

[database]
url = "sqlite::memory:"
max_connections = 1
min_connections = 1
sqlx_logging = {}
sqlx_logging_level = "{}"
"#,
        options.configured_level, options.sqlx_logging, options.sqlx_logging_level
    );
    fs::write(directory.join("config.toml"), config)
        .expect("logging test configuration must be writable");
}

fn spawn_server(directory: &Path, options: &RunOptions<'_>) -> Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_asterfsd"));
    command
        .current_dir(directory)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(rust_log) = options.rust_log {
        command.env("RUST_LOG", rust_log);
    } else {
        command.env_remove("RUST_LOG");
    }
    command.spawn().expect("AsterFSD process must start")
}

fn connect_when_ready(child: &mut Child, port: u16) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) {
            return stream;
        }
        assert!(
            child
                .try_wait()
                .expect("child status must be readable")
                .is_none(),
            "AsterFSD exited before binding its logging-test listener"
        );
        assert!(
            Instant::now() < deadline,
            "AsterFSD did not bind its logging-test listener in time"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn stop_server(mut child: Child) -> Output {
    let status = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("SIGINT helper must run");
    assert!(status.success(), "SIGINT helper must succeed");

    let deadline = Instant::now() + Duration::from_secs(5);
    while child
        .try_wait()
        .expect("child status must be readable")
        .is_none()
    {
        if Instant::now() >= deadline {
            child.kill().expect("timed-out child must be killable");
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    child
        .wait_with_output()
        .expect("AsterFSD output must be collectable")
}

fn strip_ansi(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            if characters.next_if_eq(&'[').is_some() {
                for control in characters.by_ref() {
                    if control.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        output.push(character);
    }
    output
}

fn read_connection_response(stream: &mut TcpStream) -> Vec<u8> {
    let mut response = Vec::new();
    let mut buffer = [0_u8; 1_024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => response.extend_from_slice(&buffer[..read]),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) if error.kind() == ErrorKind::ConnectionReset => break,
            Err(error) => panic!("server response must be readable: {error}"),
        }
    }
    response
}

fn run_server(options: &RunOptions<'_>) -> RunOutput {
    const MESSAGE: &[u8] = b"#TMECP1:*:logging-sentinel-message\r\n";
    const LOGIN: &[u8] = b"#APECP1:SERVER:UNKNOWN:logging-sentinel-secret:1:9:2:Logging Test\r\n";

    let directory = temp_directory();
    let port = reserve_port();
    write_config(&directory, port, options);
    let mut child = spawn_server(&directory, options);
    let mut stream = connect_when_ready(&mut child, port);
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("client timeout must be configurable");
    stream
        .write_all(MESSAGE)
        .expect("logging-test message frame must be writable");
    stream
        .write_all(LOGIN)
        .expect("logging-test login frame must be writable");
    let response = read_connection_response(&mut stream);
    drop(stream);

    let output = stop_server(child);
    let mut raw_logs = String::from_utf8_lossy(&output.stdout).into_owned();
    raw_logs.push_str(&String::from_utf8_lossy(&output.stderr));
    let logs = strip_ansi(&raw_logs);
    fs::remove_dir_all(directory).expect("logging test directory must be removable");
    assert!(
        output.status.success(),
        "AsterFSD exited unsuccessfully: {logs}"
    );
    RunOutput { logs, response }
}

#[test]
fn debug_config_emits_hot_path_logs_without_credentials() {
    let output = run_server(&RunOptions {
        configured_level: "debug",
        rust_log: None,
        sqlx_logging: false,
        sqlx_logging_level: "debug",
    });

    assert!(output.response.starts_with(b"$ERserver:ECP1:006:"));
    for marker in [
        "Logging initialized",
        "Connecting to database",
        "Initialized connection transport",
        "Prepared protocol handshake",
        "Received protocol frame",
        "Decoded classic packet envelope",
        "Decoded protocol command",
        "Executing network command",
        "Dispatching network event",
    ] {
        assert!(
            output.logs.contains(marker),
            "missing {marker}: {}",
            output.logs
        );
    }
    assert!(
        output.logs.contains("configured_filter=debug"),
        "{}",
        output.logs
    );
    assert!(!output.logs.contains("logging-sentinel-secret"));
    assert!(!output.logs.contains("logging-sentinel-message"));
    assert!(!output.logs.contains("#APECP1"));
    assert!(!output.logs.contains("sqlx::query"));
}

#[test]
fn info_config_suppresses_debug_hot_path_logs() {
    let output = run_server(&RunOptions {
        configured_level: "info",
        rust_log: None,
        sqlx_logging: false,
        sqlx_logging_level: "debug",
    });

    assert!(output.logs.contains("Logging initialized"));
    assert!(output.logs.contains("Login attempt"));
    assert!(!output.logs.contains("Received protocol frame"));
    assert!(!output.logs.contains("Decoded protocol command"));
    assert!(!output.logs.contains("logging-sentinel-secret"));
}

#[test]
fn rust_log_override_is_visible_and_takes_precedence() {
    let output = run_server(&RunOptions {
        configured_level: "debug",
        rust_log: Some("info"),
        sqlx_logging: false,
        sqlx_logging_level: "debug",
    });

    assert!(output.logs.contains("overridden by RUST_LOG"));
    assert!(
        output.logs.contains("rust_log_override=true"),
        "{}",
        output.logs
    );
    assert!(!output.logs.contains("Received protocol frame"));
    assert!(!output.logs.contains("logging-sentinel-secret"));
}

#[test]
fn invalid_rust_log_falls_back_to_configured_debug() {
    let output = run_server(&RunOptions {
        configured_level: "debug",
        rust_log: Some("aster=not-a-level"),
        sqlx_logging: false,
        sqlx_logging_level: "debug",
    });

    assert!(output.logs.contains("Invalid RUST_LOG"));
    assert!(output.logs.contains("Received protocol frame"));
    assert!(!output.logs.contains("logging-sentinel-secret"));
}

#[test]
fn sqlx_statement_logs_require_explicit_opt_in() {
    let output = run_server(&RunOptions {
        configured_level: "debug",
        rust_log: None,
        sqlx_logging: true,
        sqlx_logging_level: "debug",
    });

    assert!(output.logs.contains("sqlx::query"), "{}", output.logs);
    assert!(!output.logs.contains("logging-sentinel-secret"));
}
