use super::{
    Callsign, ClientType, Command, ConnectionId, Destination, Effects, ErrorCode, Event, Network,
    NetworkState, Position, QueryKind, RangePolicy, SessionPhase,
};

impl Network {
    pub(super) async fn execute_active(
        &self,
        connection_id: ConnectionId,
        command: Command,
    ) -> Effects {
        let mut state = self.state.write().await;
        let source = command.source().cloned();
        let (callsign, client_type, rating) = {
            let Some(session) = state.sessions.get(&connection_id) else {
                return Effects::default();
            };
            if session.phase != SessionPhase::Active {
                return Effects::default();
            }
            let Some(callsign) = session.callsign().cloned() else {
                return Effects::default();
            };
            let Some(presence) = session.presence.as_ref() else {
                return Effects::default();
            };
            (callsign, presence.client_type, presence.rating)
        };
        if source.as_ref() != Some(&callsign) {
            return Self::error_effect(
                connection_id,
                Some(callsign),
                ErrorCode::InvalidSource,
                source.map_or_else(String::new, |callsign| callsign.to_string()),
            );
        }

        match command {
            command @ (Command::Logoff { .. } | Command::Position(_) | Command::FlightPlan(_)) => {
                Self::handle_state_command(
                    &mut state,
                    connection_id,
                    callsign,
                    client_type,
                    command,
                )
            }
            Command::Query {
                source,
                destination,
                kind,
                arguments,
            } => Self::handle_query(&state, connection_id, source, destination, kind, arguments),
            Command::Ping {
                source,
                destination,
                payload,
            } => Self::handle_ping(&state, connection_id, source, destination, payload),
            command @ (Command::Text { .. }
            | Command::Response { .. }
            | Command::Pong { .. }
            | Command::Handoff { .. }
            | Command::ClientData { .. }) => {
                Self::handle_routed_command(&state, connection_id, command)
            }
            Command::Kill {
                source: _,
                target,
                reason,
            } => Self::handle_kill(
                &mut state,
                connection_id,
                callsign,
                rating,
                &target,
                &reason,
            ),
            Command::Noop { .. }
            | Command::Identify(_)
            | Command::Login(_)
            | Command::WeatherRequest { .. } => Effects::default(),
        }
    }

    fn handle_state_command(
        state: &mut NetworkState,
        connection_id: ConnectionId,
        callsign: Callsign,
        client_type: ClientType,
        command: Command,
    ) -> Effects {
        match command {
            Command::Logoff { .. } => {
                Self::disconnect_locked(state, connection_id, "client logoff")
            }
            Command::Position(position) => {
                if let Err(error) = position.validate() {
                    return Self::error_effect(
                        connection_id,
                        Some(callsign),
                        ErrorCode::Syntax,
                        error.to_string(),
                    );
                }
                let expected_type = match position {
                    Position::Pilot(_) => ClientType::Pilot,
                    Position::Atc(_) => ClientType::Atc,
                };
                if client_type != expected_type {
                    return Self::error_effect(
                        connection_id,
                        Some(callsign),
                        ErrorCode::Syntax,
                        "position type".to_string(),
                    );
                }
                let Some(session) = state.sessions.get_mut(&connection_id) else {
                    return Effects::default();
                };
                session.position = Some(position.clone());
                let recipients = Self::position_recipients(state, connection_id);
                Self::send(recipients, Event::Position { position })
            }
            Command::FlightPlan(plan) => {
                let Some(session) = state.sessions.get_mut(&connection_id) else {
                    return Effects::default();
                };
                session.flight_plan = Some(plan.clone());
                let recipients = Self::flight_plan_recipients(state, connection_id, 400.0);
                Self::send(
                    recipients,
                    Event::FlightPlan {
                        plan,
                        destination: Destination::Atc,
                    },
                )
            }
            _ => Effects::default(),
        }
    }

    fn handle_ping(
        state: &NetworkState,
        connection_id: ConnectionId,
        source: Callsign,
        destination: Destination,
        payload: String,
    ) -> Effects {
        if destination == Destination::Server {
            return Self::send(
                vec![connection_id],
                Event::Pong {
                    source: "server".to_string(),
                    destination: Destination::Direct(source),
                    payload,
                },
            );
        }
        let recipients =
            Self::resolve_destination(state, connection_id, &destination, RangePolicy::Source);
        Self::send(
            recipients,
            Event::Ping {
                source,
                destination,
                payload,
            },
        )
    }

    fn handle_routed_command(
        state: &NetworkState,
        connection_id: ConnectionId,
        command: Command,
    ) -> Effects {
        let (destination, range_policy, event) = match command {
            Command::Text {
                source,
                destination,
                message,
            } => (
                destination.clone(),
                RangePolicy::Message,
                Event::Text {
                    source,
                    destination,
                    message,
                },
            ),
            Command::Response {
                source,
                destination,
                kind,
                arguments,
            } => (
                destination.clone(),
                RangePolicy::Source,
                Event::Response {
                    source: source.to_string(),
                    destination,
                    kind,
                    arguments,
                },
            ),
            Command::Pong {
                source,
                destination,
                payload,
            } => (
                destination.clone(),
                RangePolicy::Source,
                Event::Pong {
                    source: source.to_string(),
                    destination,
                    payload,
                },
            ),
            Command::Handoff {
                source,
                target,
                kind,
                fields,
            } => {
                let destination = Destination::Direct(target.clone());
                (
                    destination,
                    RangePolicy::Source,
                    Event::Handoff {
                        source,
                        target,
                        kind,
                        fields,
                    },
                )
            }
            Command::ClientData {
                source,
                target,
                kind,
                fields,
            } => {
                let destination = Destination::Direct(target.clone());
                (
                    destination,
                    RangePolicy::Source,
                    Event::ClientData {
                        source,
                        target,
                        kind,
                        fields,
                    },
                )
            }
            _ => return Effects::default(),
        };
        let recipients =
            Self::resolve_destination(state, connection_id, &destination, range_policy);
        Self::send(recipients, event)
    }

    fn handle_kill(
        state: &mut NetworkState,
        connection_id: ConnectionId,
        callsign: Callsign,
        rating: i32,
        target: &Callsign,
        reason: &str,
    ) -> Effects {
        let Some(target_id) = state.callsigns.get(target).copied() else {
            return Self::error_effect(
                connection_id,
                Some(callsign),
                ErrorCode::NoSuchCallsign,
                target.to_string(),
            );
        };
        if rating < 11 {
            return Self::send(
                vec![connection_id],
                Event::Welcome {
                    callsign,
                    message: "You are not allowed to kill users!".to_string(),
                },
            );
        }
        let mut effects = Self::send(
            vec![connection_id],
            Event::Welcome {
                callsign,
                message: format!("Attempting to kill {target}"),
            },
        );
        effects.extend(Self::send(
            vec![target_id],
            Event::Disconnect {
                target: target.clone(),
                reason: reason.to_string(),
            },
        ));
        effects.extend(Self::disconnect_locked(state, target_id, reason));
        effects
    }

    fn handle_query(
        state: &NetworkState,
        connection_id: ConnectionId,
        source: Callsign,
        destination: Destination,
        kind: QueryKind,
        arguments: Vec<String>,
    ) -> Effects {
        if destination != Destination::Server {
            let recipients =
                Self::resolve_destination(state, connection_id, &destination, RangePolicy::Source);
            return Self::send(
                recipients,
                Event::Query {
                    source: source.to_string(),
                    destination,
                    kind,
                    arguments,
                },
            );
        }
        match kind {
            QueryKind::FlightPlan => {
                let target = arguments
                    .first()
                    .and_then(|value| Callsign::parse(value).ok());
                let Some(target) = target else {
                    return Self::error_effect(
                        connection_id,
                        Some(source),
                        ErrorCode::Syntax,
                        String::new(),
                    );
                };
                let Some(target_id) = state.callsigns.get(&target) else {
                    return Self::error_effect(
                        connection_id,
                        Some(source),
                        ErrorCode::NoSuchCallsign,
                        target.to_string(),
                    );
                };
                let Some(plan) = state
                    .sessions
                    .get(target_id)
                    .and_then(|session| session.flight_plan.clone())
                else {
                    return Self::error_effect(
                        connection_id,
                        Some(source),
                        ErrorCode::NoFlightPlan,
                        String::new(),
                    );
                };
                Self::send(
                    vec![connection_id],
                    Event::FlightPlan {
                        plan,
                        destination: Destination::Direct(source),
                    },
                )
            }
            QueryKind::RealName
            | QueryKind::Capabilities
            | QueryKind::Atis
            | QueryKind::SystemInfo
            | QueryKind::AircraftConfiguration
            | QueryKind::Raw(_) => Effects::default(),
        }
    }
}
