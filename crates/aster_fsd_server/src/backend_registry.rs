use aster_fsd_model::ProtocolDialect;
use aster_fsd_protocol::ProtocolBackend;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry mapping each configured dialect to exactly one backend.
#[derive(Default)]
pub struct BackendRegistry {
    backends: HashMap<ProtocolDialect, Arc<dyn ProtocolBackend>>,
}

impl BackendRegistry {
    /// Adds or replaces the backend for its declared dialect.
    pub fn register(&mut self, backend: Arc<dyn ProtocolBackend>) {
        self.backends.insert(backend.dialect(), backend);
    }

    pub(crate) fn get(&self, dialect: ProtocolDialect) -> Option<Arc<dyn ProtocolBackend>> {
        self.backends.get(&dialect).cloned()
    }
}
