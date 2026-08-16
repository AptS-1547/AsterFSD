use aster_fsd_model::{
    Callsign, ClientPresence, ConnectionId, FlightPlan, Identification, Position, ProtocolDialect,
    SessionPhase,
};
use std::collections::HashMap;
use std::net::SocketAddr;

/// Read-only session data exposed to the transport supervisor.
#[derive(Debug, Clone)]
pub struct SessionSnapshot {
    pub connection_id: ConnectionId,
    pub peer: SocketAddr,
    pub dialect: ProtocolDialect,
    pub phase: SessionPhase,
    pub callsign: Option<Callsign>,
    pub presence: Option<ClientPresence>,
}

/// Failure to register a newly accepted connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterError {
    ServerFull,
    DuplicateConnection,
}

#[derive(Debug)]
pub(super) struct Session {
    pub(super) connection_id: ConnectionId,
    pub(super) peer: SocketAddr,
    pub(super) dialect: ProtocolDialect,
    pub(super) phase: SessionPhase,
    pub(super) generation: u64,
    pub(super) identification: Option<Identification>,
    pub(super) presence: Option<ClientPresence>,
    pub(super) position: Option<Position>,
    pub(super) flight_plan: Option<FlightPlan>,
}

impl Session {
    pub(super) fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            connection_id: self.connection_id,
            peer: self.peer,
            dialect: self.dialect,
            phase: self.phase,
            callsign: self
                .presence
                .as_ref()
                .map(|presence| presence.callsign.clone())
                .or_else(|| {
                    self.identification
                        .as_ref()
                        .map(|identification| identification.callsign.clone())
                }),
            presence: self.presence.clone(),
        }
    }

    pub(super) fn callsign(&self) -> Option<&Callsign> {
        self.presence.as_ref().map(|presence| &presence.callsign)
    }
}

#[derive(Debug, Default)]
pub(super) struct NetworkState {
    pub(super) sessions: HashMap<ConnectionId, Session>,
    pub(super) callsigns: HashMap<Callsign, ConnectionId>,
}
