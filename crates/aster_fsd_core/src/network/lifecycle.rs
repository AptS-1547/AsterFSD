use super::{
    Callsign, CloseConnection, ConnectionId, Delivery, Effects, ErrorCode, Event, Network,
    NetworkState, SessionPhase,
};

impl Network {
    /// Removes a connection and releases all authoritative indexes.
    pub async fn disconnect(&self, connection_id: ConnectionId, reason: &str) -> Effects {
        let mut state = self.state.write().await;
        Self::disconnect_locked(&mut state, connection_id, reason)
    }

    pub(super) fn disconnect_locked(
        state: &mut NetworkState,
        connection_id: ConnectionId,
        reason: &str,
    ) -> Effects {
        let Some(mut session) = state.sessions.remove(&connection_id) else {
            return Effects::default();
        };
        session.phase = SessionPhase::Closed;
        let mut effects = Effects::default();
        if let Some(presence) = session.presence {
            state.callsigns.remove(&presence.callsign);
            let peers = Self::active_ids(state, None);
            if !peers.is_empty() {
                effects.deliveries.push(Delivery {
                    recipients: peers,
                    event: Event::ClientRemoved {
                        callsign: presence.callsign,
                        client_type: presence.client_type,
                        network_id: presence.network_id,
                    },
                });
            }
        }
        effects.close = Some(CloseConnection {
            connection_id,
            reason: reason.to_string(),
        });
        effects
    }

    pub(super) fn error_effect(
        connection_id: ConnectionId,
        callsign: Option<Callsign>,
        code: ErrorCode,
        environment: String,
    ) -> Effects {
        Self::send(
            vec![connection_id],
            Event::Error {
                callsign,
                code,
                environment,
                description: code.description().to_string(),
            },
        )
    }

    pub(super) fn error_and_close(
        connection_id: ConnectionId,
        callsign: Option<Callsign>,
        code: ErrorCode,
        environment: String,
    ) -> Effects {
        let mut effects = Self::error_effect(connection_id, callsign, code, environment);
        effects.close = Some(CloseConnection {
            connection_id,
            reason: code.description().to_string(),
        });
        effects
    }

    pub(super) fn send(recipients: Vec<ConnectionId>, event: Event) -> Effects {
        if recipients.is_empty() {
            Effects::default()
        } else {
            Effects {
                deliveries: vec![Delivery { recipients, event }],
                close: None,
            }
        }
    }
}
