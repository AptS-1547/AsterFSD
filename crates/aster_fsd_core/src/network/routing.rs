use super::{
    ClientType, ConnectionId, Destination, Network, NetworkState, Position, RangePolicy, Session,
    SessionPhase,
};

impl Network {
    pub(super) fn active_ids(
        state: &NetworkState,
        exclude: Option<ConnectionId>,
    ) -> Vec<ConnectionId> {
        state
            .sessions
            .values()
            .filter(|session| {
                session.phase == SessionPhase::Active && Some(session.connection_id) != exclude
            })
            .map(|session| session.connection_id)
            .collect()
    }

    pub(super) fn resolve_destination(
        state: &NetworkState,
        sender: ConnectionId,
        destination: &Destination,
        range_policy: RangePolicy,
    ) -> Vec<ConnectionId> {
        let recipients = match destination {
            Destination::Server => Vec::new(),
            Destination::Direct(callsign) => state
                .callsigns
                .get(callsign)
                .copied()
                .filter(|connection_id| *connection_id != sender)
                .into_iter()
                .collect(),
            Destination::All => Self::active_ids(state, Some(sender)),
            Destination::Atc => state
                .sessions
                .values()
                .filter(|session| {
                    session.connection_id != sender
                        && session.phase == SessionPhase::Active
                        && session
                            .presence
                            .as_ref()
                            .is_some_and(|presence| presence.client_type == ClientType::Atc)
                })
                .map(|session| session.connection_id)
                .collect(),
            Destination::Pilots => state
                .sessions
                .values()
                .filter(|session| {
                    session.connection_id != sender
                        && session.phase == SessionPhase::Active
                        && session.presence.as_ref().is_some_and(|presence| {
                            matches!(
                                presence.client_type,
                                ClientType::Pilot | ClientType::Observer
                            )
                        })
                })
                .map(|session| session.connection_id)
                .collect(),
            Destination::Range(_) => state
                .sessions
                .values()
                .filter(|session| {
                    session.connection_id != sender
                        && session.phase == SessionPhase::Active
                        && match range_policy {
                            RangePolicy::Source => {
                                Self::within_source_range(state, sender, session.connection_id)
                            }
                            RangePolicy::Message => {
                                Self::within_message_range(state, sender, session.connection_id)
                            }
                        }
                })
                .map(|session| session.connection_id)
                .collect(),
        };
        tracing::trace!(
            sender = %sender,
            ?destination,
            recipients = recipients.len(),
            "Resolved command destination"
        );
        recipients
    }

    pub(super) fn position_recipients(
        state: &NetworkState,
        source: ConnectionId,
    ) -> Vec<ConnectionId> {
        state
            .sessions
            .values()
            .filter(|target| {
                target.connection_id != source
                    && target.phase == SessionPhase::Active
                    && Self::within_position_range(state, source, target.connection_id)
            })
            .map(|session| session.connection_id)
            .collect()
    }

    pub(super) fn flight_plan_recipients(
        state: &NetworkState,
        source: ConnectionId,
        range: f64,
    ) -> Vec<ConnectionId> {
        state
            .sessions
            .values()
            .filter(|target| {
                target.connection_id != source
                    && target.phase == SessionPhase::Active
                    && target
                        .presence
                        .as_ref()
                        .is_some_and(|presence| presence.client_type == ClientType::Atc)
                    && Self::distance_between(state, source, target.connection_id)
                        .is_some_and(|distance| distance <= range)
            })
            .map(|session| session.connection_id)
            .collect()
    }

    pub(super) fn within_position_range(
        state: &NetworkState,
        source_id: ConnectionId,
        target_id: ConnectionId,
    ) -> bool {
        let (Some(source), Some(target)) = (
            state.sessions.get(&source_id),
            state.sessions.get(&target_id),
        ) else {
            return false;
        };
        let Some(distance) = Self::distance_between(state, source_id, target_id) else {
            return false;
        };
        let source_range = Self::session_range(source);
        let target_range = Self::session_range(target);
        let target_type = target
            .presence
            .as_ref()
            .map(|presence| presence.client_type);
        let source_type = source
            .presence
            .as_ref()
            .map(|presence| presence.client_type);
        let permitted = if target_type == Some(ClientType::Atc) {
            match target.position.as_ref() {
                Some(Position::Atc(position)) => f64::from(position.visual_range.max(0)),
                _ => target_range,
            }
        } else if matches!(source_type, Some(ClientType::Pilot | ClientType::Observer)) {
            source_range + target_range
        } else {
            source_range.max(target_range)
        };
        distance <= permitted
    }

    pub(super) fn within_message_range(
        state: &NetworkState,
        source_id: ConnectionId,
        target_id: ConnectionId,
    ) -> bool {
        let (Some(source), Some(target), Some(distance)) = (
            state.sessions.get(&source_id),
            state.sessions.get(&target_id),
            Self::distance_between(state, source_id, target_id),
        ) else {
            return false;
        };
        let source_range = Self::session_range(source);
        let target_range = Self::session_range(target);
        let both_pilot = source.presence.as_ref().is_some_and(|presence| {
            matches!(
                presence.client_type,
                ClientType::Pilot | ClientType::Observer
            )
        }) && target.presence.as_ref().is_some_and(|presence| {
            matches!(
                presence.client_type,
                ClientType::Pilot | ClientType::Observer
            )
        });
        distance
            <= if both_pilot {
                source_range + target_range
            } else {
                source_range.max(target_range)
            }
    }

    pub(super) fn within_source_range(
        state: &NetworkState,
        source_id: ConnectionId,
        target_id: ConnectionId,
    ) -> bool {
        let (Some(source), Some(distance)) = (
            state.sessions.get(&source_id),
            Self::distance_between(state, source_id, target_id),
        ) else {
            return false;
        };
        distance <= Self::session_range(source)
    }

    pub(super) fn session_range(session: &Session) -> f64 {
        match (&session.presence, &session.position) {
            (Some(presence), Some(position))
                if matches!(
                    presence.client_type,
                    ClientType::Pilot | ClientType::Observer
                ) =>
            {
                (10.0 + 1.414 * f64::from(position.altitude().max(0)).sqrt()).trunc()
            }
            (_, Some(Position::Atc(position))) => match position.facility_type {
                1 | 7 => 1_500.0,
                2 | 3 => 5.0,
                4 => 30.0,
                5 => 100.0,
                6 => 400.0,
                _ => 40.0,
            },
            _ => 40.0,
        }
    }

    pub(super) fn distance_between(
        state: &NetworkState,
        source: ConnectionId,
        target: ConnectionId,
    ) -> Option<f64> {
        let source = state.sessions.get(&source)?.position.as_ref()?;
        let target = state.sessions.get(&target)?.position.as_ref()?;
        let (source_latitude, source_longitude) = source.coordinates();
        let (target_latitude, target_longitude) = target.coordinates();
        let source_latitude = source_latitude.to_radians();
        let target_latitude = target_latitude.to_radians();
        let latitude_delta = target_latitude - source_latitude;
        let longitude_delta = (target_longitude - source_longitude).to_radians();
        let a = (latitude_delta / 2.0).sin().powi(2)
            + source_latitude.cos() * target_latitude.cos() * (longitude_delta / 2.0).sin().powi(2);
        Some(3_440.065 * 2.0 * a.sqrt().atan2((1.0 - a).sqrt()))
    }
}
