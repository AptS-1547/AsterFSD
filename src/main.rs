//! `AsterFSD` production composition root.

#![cfg_attr(
    not(test),
    deny(
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unreachable,
        clippy::unwrap_used
    )
)]

use aster_fsd_core::Network;
use aster_fsd_persistence::{SeaOrmAuthenticator, connect};
use aster_fsd_protocol_aster::AsterProtocolV1;
use aster_fsd_protocol_classic::ClassicProtocol;
use aster_fsd_protocol_vatsim::VatsimProtocol;
use aster_fsd_server::{BackendRegistry, Server};
use asterfsd::config::Config;
use std::path::Path;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (config, config_source) = if Path::new("config.toml").exists() {
        (Config::from_file("config.toml")?, "config.toml")
    } else {
        eprintln!("config.toml not found; using default configuration");
        (Config::default(), "defaults")
    };
    config.validate()?;

    let effective_logging = config.effective_logging_config();
    let logging = aster_forge_logging::init_logging(&effective_logging);
    let _logging_guard = logging.guard;
    if let Some(warning) = logging.warning {
        tracing::warn!(warning = %warning, "Logging configuration warning");
    }

    tracing::info!(
        configured_filter = %config.logging.level,
        effective_filter = %effective_logging.level,
        format = %effective_logging.format,
        output = if effective_logging.file.is_empty() { "stdout" } else { "file" },
        rust_log_override = std::env::var_os("RUST_LOG").is_some(),
        "Logging initialized"
    );

    tracing::info!(
        version = %config.server.version,
        config_source,
        listeners = config.listeners.len(),
        "Starting AsterFSD"
    );
    let database = connect(&config.runtime_database_config()).await?;
    let authenticator = Arc::new(SeaOrmAuthenticator::new(database));
    let network = Arc::new(Network::new(config.runtime_core_config(), authenticator));

    let mut backends = BackendRegistry::default();
    backends.register(Arc::new(ClassicProtocol));
    backends.register(Arc::new(VatsimProtocol::default()));
    backends.register(Arc::new(AsterProtocolV1));

    let bound = Server::new(config.runtime_server_config(), network, backends)?
        .bind()
        .await?;
    let shutdown = CancellationToken::new();
    let signal_shutdown = shutdown.clone();
    let signal = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal_shutdown.cancel();
        }
    });
    let result = bound.serve(shutdown.clone()).await;
    shutdown.cancel();
    signal.abort();
    let _ = signal.await;
    result?;
    Ok(())
}
