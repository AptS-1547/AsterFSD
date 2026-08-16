use super::{
    AuthError, AuthenticatedIdentity, CLASSIC_PROTOCOL_REVISION, ClientPresence, ClientType,
    ConnectionId, Delivery, Destination, Effects, ErrorCode, Event, LoginCommand, Network,
    NetworkState, ProtocolDialect, QueryKind, SessionPhase, SocketAddr, VATSIM_PROTOCOL_REVISION,
};

impl Network {
    pub(super) async fn identify(
        &self,
        connection_id: ConnectionId,
        command: aster_fsd_model::Identification,
    ) -> Effects {
        tracing::debug!(
            %connection_id,
            callsign = %command.callsign,
            client_id = %command.client_id,
            "Processing client identification"
        );
        let generation = {
            let state = self.state.read().await;
            let Some(session) = state.sessions.get(&connection_id) else {
                return Effects::default();
            };
            if session.phase != SessionPhase::Connected {
                return Self::error_effect(
                    connection_id,
                    session.callsign().cloned(),
                    ErrorCode::AlreadyRegistered,
                    String::new(),
                );
            }
            session.generation
        };

        if let Err(error) = self
            .authenticator
            .authorize_client(&command.client_id)
            .await
        {
            tracing::warn!(
                connection_id = %connection_id,
                client_id = %command.client_id,
                error = %error,
                "Client software authorization failed"
            );
            return Self::error_and_close(
                connection_id,
                Some(command.callsign),
                ErrorCode::UnauthorizedClient,
                command.client_id,
            );
        }

        let mut state = self.state.write().await;
        let Some(session) = state.sessions.get_mut(&connection_id) else {
            return Effects::default();
        };
        if session.phase != SessionPhase::Connected || session.generation != generation {
            return Self::error_effect(
                connection_id,
                session.callsign().cloned(),
                ErrorCode::AlreadyRegistered,
                String::new(),
            );
        }
        session.identification = Some(command);
        session.phase = SessionPhase::Identified;
        session.generation += 1;
        tracing::debug!(
            %connection_id,
            generation = session.generation,
            "Client identification accepted"
        );
        Effects::default()
    }

    pub(super) async fn login(
        &self,
        connection_id: ConnectionId,
        command: LoginCommand,
    ) -> Effects {
        tracing::info!(
            %connection_id,
            callsign = %command.callsign,
            network_id = %command.network_id,
            client_type = ?command.client_type,
            requested_rating = command.requested_rating,
            protocol_revision = command.protocol_revision,
            "Login attempt"
        );
        let generation = match self.login_generation(connection_id, &command).await {
            Ok(generation) => generation,
            Err(effects) => return effects,
        };

        let authenticated = match self
            .authenticator
            .authenticate(&command.network_id, &command.password)
            .await
        {
            Ok(identity) => identity,
            Err(AuthError::Suspended) => {
                return Self::error_and_close(
                    connection_id,
                    Some(command.callsign),
                    ErrorCode::Suspended,
                    String::new(),
                );
            }
            Err(error) => {
                tracing::warn!(
                    connection_id = %connection_id,
                    network_id = %command.network_id,
                    error = %error,
                    "Login authentication failed"
                );
                return Self::error_and_close(
                    connection_id,
                    Some(command.callsign),
                    ErrorCode::InvalidCredentials,
                    command.network_id,
                );
            }
        };
        tracing::debug!(
            %connection_id,
            network_id = %authenticated.network_id,
            suspended = authenticated.suspended,
            atc_rating = ?authenticated.atc_rating,
            pilot_rating = ?authenticated.pilot_rating,
            "Authentication provider accepted identity"
        );

        if let Err(effects) = Self::validate_rating(connection_id, &command, &authenticated) {
            return effects;
        }

        let mut state = self.state.write().await;
        self.activate_login(
            &mut state,
            connection_id,
            command,
            authenticated,
            generation,
        )
    }

    async fn login_generation(
        &self,
        connection_id: ConnectionId,
        command: &LoginCommand,
    ) -> Result<u64, Effects> {
        let state = self.state.read().await;
        let Some(session) = state.sessions.get(&connection_id) else {
            return Err(Effects::default());
        };
        if session.phase == SessionPhase::Active {
            return Err(Self::error_effect(
                connection_id,
                session.callsign().cloned(),
                ErrorCode::AlreadyRegistered,
                String::new(),
            ));
        }
        if session.dialect == ProtocolDialect::Vatsim && session.phase != SessionPhase::Identified {
            return Err(Self::error_and_close(
                connection_id,
                Some(command.callsign.clone()),
                ErrorCode::Syntax,
                "missing $ID".to_string(),
            ));
        }
        let expected_revision = match session.dialect {
            ProtocolDialect::Classic => CLASSIC_PROTOCOL_REVISION,
            ProtocolDialect::AsterV1 => 1,
            ProtocolDialect::Vatsim => VATSIM_PROTOCOL_REVISION,
        };
        if command.protocol_revision != expected_revision {
            return Err(Self::error_and_close(
                connection_id,
                Some(command.callsign.clone()),
                ErrorCode::InvalidProtocolRevision,
                String::new(),
            ));
        }
        if state.callsigns.contains_key(&command.callsign) {
            return Err(Self::error_and_close(
                connection_id,
                Some(command.callsign.clone()),
                ErrorCode::CallsignInUse,
                String::new(),
            ));
        }
        Ok(session.generation)
    }

    fn validate_rating(
        connection_id: ConnectionId,
        command: &LoginCommand,
        authenticated: &AuthenticatedIdentity,
    ) -> Result<(), Effects> {
        let identity_is_active = match command.client_type {
            ClientType::Atc => authenticated.atc_rating != aster_fsd_model::AtcRating::Suspended,
            ClientType::Pilot | ClientType::Observer => {
                authenticated.pilot_rating != aster_fsd_model::PilotRating::Unrated
            }
        };
        if authenticated.suspended || !identity_is_active {
            return Err(Self::error_and_close(
                connection_id,
                Some(command.callsign.clone()),
                ErrorCode::Suspended,
                String::new(),
            ));
        }
        let rating_is_authorized = match command.client_type {
            ClientType::Atc => authenticated
                .atc_rating
                .allows_wire_value(command.requested_rating),
            ClientType::Pilot | ClientType::Observer => authenticated
                .pilot_rating
                .allows_wire_value(command.requested_rating),
        };
        if !rating_is_authorized {
            return Err(Self::error_and_close(
                connection_id,
                Some(command.callsign.clone()),
                ErrorCode::RequestedLevelTooHigh,
                command.requested_rating.to_string(),
            ));
        }
        Ok(())
    }

    fn activate_login(
        &self,
        state: &mut NetworkState,
        connection_id: ConnectionId,
        command: LoginCommand,
        authenticated: AuthenticatedIdentity,
        generation: u64,
    ) -> Effects {
        if state.callsigns.contains_key(&command.callsign) {
            return Self::error_and_close(
                connection_id,
                Some(command.callsign),
                ErrorCode::CallsignInUse,
                String::new(),
            );
        }
        let Some(session) = state.sessions.get_mut(&connection_id) else {
            return Effects::default();
        };
        if !matches!(
            session.phase,
            SessionPhase::Connected | SessionPhase::Identified
        ) || session.generation != generation
        {
            return Self::error_effect(
                connection_id,
                session.callsign().cloned(),
                ErrorCode::AlreadyRegistered,
                String::new(),
            );
        }
        if session.dialect == ProtocolDialect::Vatsim
            && session
                .identification
                .as_ref()
                .is_none_or(|identification| {
                    identification.callsign != command.callsign
                        || identification.network_id.as_deref() != Some(command.network_id.as_str())
                })
        {
            return Self::error_and_close(
                connection_id,
                Some(command.callsign),
                ErrorCode::InvalidSource,
                String::new(),
            );
        }

        let presence = ClientPresence {
            callsign: command.callsign.clone(),
            client_type: command.client_type,
            network_id: authenticated.network_id,
            real_name: authenticated.real_name,
            rating: command.requested_rating,
            protocol_revision: command.protocol_revision,
            simulator_type: command.simulator_type,
        };
        session.phase = SessionPhase::Active;
        session.presence = Some(presence.clone());
        session.generation += 1;
        state.callsigns.insert(command.callsign, connection_id);

        tracing::info!(
            %connection_id,
            callsign = %presence.callsign,
            network_id = %presence.network_id,
            client_type = ?presence.client_type,
            rating = presence.rating,
            "Login successful"
        );

        self.login_effects(state, connection_id, &presence)
    }

    fn login_effects(
        &self,
        state: &NetworkState,
        connection_id: ConnectionId,
        presence: &ClientPresence,
    ) -> Effects {
        let peers = Self::active_ids(state, Some(connection_id));
        let mut effects = Effects::default();
        if !peers.is_empty() {
            effects.deliveries.push(Delivery {
                recipients: peers,
                event: Event::ClientAdded {
                    client: presence.clone(),
                },
            });
        }
        effects.deliveries.push(Delivery {
            recipients: vec![connection_id],
            event: Event::Welcome {
                callsign: presence.callsign.clone(),
                message: self.config.product_message.clone(),
            },
        });
        for line in &self.config.motd {
            effects.deliveries.push(Delivery {
                recipients: vec![connection_id],
                event: Event::Welcome {
                    callsign: presence.callsign.clone(),
                    message: line.clone(),
                },
            });
        }
        if let Some(session) = state
            .sessions
            .get(&connection_id)
            .filter(|session| session.dialect == ProtocolDialect::Vatsim)
        {
            Self::append_vatsim_login_profile(&mut effects, connection_id, session.peer, presence);
        }
        effects
    }

    fn append_vatsim_login_profile(
        effects: &mut Effects,
        connection_id: ConnectionId,
        peer: SocketAddr,
        presence: &ClientPresence,
    ) {
        let destination = Destination::Direct(presence.callsign.clone());
        effects.deliveries.push(Delivery {
            recipients: vec![connection_id],
            event: Event::Query {
                source: "SERVER".to_string(),
                destination: destination.clone(),
                kind: QueryKind::Capabilities,
                arguments: Vec::new(),
            },
        });

        if presence.client_type == ClientType::Atc {
            effects.deliveries.push(Delivery {
                recipients: vec![connection_id],
                event: Event::Response {
                    source: "SERVER".to_string(),
                    destination: destination.clone(),
                    kind: QueryKind::Raw("ATC".to_string()),
                    arguments: vec!["N".to_string(), presence.callsign.to_string()],
                },
            });
            effects.deliveries.push(Delivery {
                recipients: vec![connection_id],
                event: Event::Response {
                    source: "SERVER".to_string(),
                    destination: destination.clone(),
                    kind: QueryKind::Capabilities,
                    arguments: vec!["ATCINFO=1".to_string(), "SECPOS=1".to_string()],
                },
            });
        }

        effects.deliveries.push(Delivery {
            recipients: vec![connection_id],
            event: Event::Response {
                source: "SERVER".to_string(),
                destination,
                kind: QueryKind::Raw("IP".to_string()),
                arguments: vec![peer.ip().to_string()],
            },
        });

        if matches!(
            presence.client_type,
            ClientType::Pilot | ClientType::Observer
        ) {
            effects.deliveries.push(Delivery {
                recipients: vec![connection_id],
                event: Event::Error {
                    callsign: Some(presence.callsign.clone()),
                    code: ErrorCode::NoFlightPlan,
                    environment: presence.callsign.to_string(),
                    description: ErrorCode::NoFlightPlan.description().to_string(),
                },
            });
        }
    }
}
