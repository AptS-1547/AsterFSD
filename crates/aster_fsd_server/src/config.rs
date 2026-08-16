use aster_fsd_model::ProtocolDialect;

/// Configuration for one explicitly selected protocol listener.
#[derive(Debug, Clone)]
pub struct ListenerConfig {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub protocol: ProtocolDialect,
    pub max_frame_bytes: usize,
    pub idle_timeout_seconds: u64,
}

/// Transport-level server configuration shared by all listeners.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub server_name: String,
    pub server_version: String,
    pub mailbox_capacity: usize,
    pub wind_delta_interval_seconds: u64,
    pub listeners: Vec<ListenerConfig>,
}
