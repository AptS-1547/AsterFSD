use aster_fsd_model::{ConnectionId, Event};

/// One event and its fully resolved recipient connection IDs.
///
/// The event is stored inline. Fan-out borrows it and the server caches encoded
/// frames per dialect instead of adding a `Box` allocation to every delivery.
#[derive(Debug, Clone)]
pub struct Delivery {
    pub recipients: Vec<ConnectionId>,
    pub event: Event,
}

/// Direct transport close requested after semantic effects are delivered.
#[derive(Debug, Clone)]
pub struct CloseConnection {
    pub connection_id: ConnectionId,
    pub reason: String,
}

/// Ordered semantic output of executing one command.
#[derive(Debug, Clone, Default)]
pub struct Effects {
    pub deliveries: Vec<Delivery>,
    pub close: Option<CloseConnection>,
}

impl Effects {
    /// Returns whether the command produced neither delivery nor close control.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.deliveries.is_empty() && self.close.is_none()
    }

    pub(crate) fn extend(&mut self, mut other: Self) {
        self.deliveries.append(&mut other.deliveries);
        if other.close.is_some() {
            debug_assert!(
                self.close.is_none(),
                "one command closes at most one connection"
            );
            self.close = other.close;
        }
    }
}
