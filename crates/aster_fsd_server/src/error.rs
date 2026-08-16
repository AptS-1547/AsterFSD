use thiserror::Error;

/// Listener, binding and supervision failures surfaced by the runtime.
#[derive(Debug, Error)]
pub enum ServerError {
    #[error("listener {listener} has no registered protocol backend")]
    MissingBackend { listener: String },
    #[error("listener {listener} failed: {source}")]
    ListenerIo {
        listener: String,
        #[source]
        source: std::io::Error,
    },
    #[error("listener task failed: {0}")]
    ListenerTask(String),
    #[error("mailbox capacity must be greater than zero")]
    InvalidMailboxCapacity,
    #[error("listener and maintenance intervals must be greater than zero")]
    InvalidInterval,
    #[error("at least one listener is required")]
    NoListeners,
}
