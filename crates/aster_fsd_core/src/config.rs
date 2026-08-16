/// Runtime policy owned by the shared network core.
#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub max_clients: usize,
    pub product_message: String,
    pub motd: Vec<String>,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            max_clients: 1_000,
            product_message: format!("AsterFSD {}", env!("CARGO_PKG_VERSION")),
            motd: Vec::new(),
        }
    }
}
