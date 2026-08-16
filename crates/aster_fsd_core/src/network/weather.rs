use super::{
    Callsign, ConnectionId, Effects, ErrorCode, Event, Network, Position, SessionPhase,
    WeatherLookup, WeatherProfile,
};

impl Network {
    pub(super) async fn weather_request(
        &self,
        connection_id: ConnectionId,
        source: Callsign,
        station: String,
        parsed: bool,
    ) -> Effects {
        let coordinates = match self
            .weather_request_coordinates(connection_id, &source)
            .await
        {
            Ok(coordinates) => coordinates,
            Err(effects) => return effects,
        };
        tracing::debug!(
            %connection_id,
            %source,
            %station,
            parsed,
            has_position = coordinates.is_some(),
            "Resolving weather request"
        );
        let request = WeatherLookup {
            station: station.clone(),
            coordinates,
        };
        let observation = match self.weather_provider.lookup(&request).await {
            Ok(Some(observation)) => observation,
            Ok(None) => {
                tracing::debug!(%connection_id, %station, "Weather station was not found");
                return Self::no_weather(connection_id, source, station);
            }
            Err(_error) => {
                tracing::warn!(
                    %connection_id,
                    %station,
                    error_category = "provider",
                    "Weather provider lookup failed"
                );
                return Self::no_weather(connection_id, source, station);
            }
        };
        if parsed {
            Self::parsed_weather_effect(connection_id, source, station, observation.profile)
        } else {
            Self::raw_weather_effect(connection_id, source, station, observation.raw_metar)
        }
    }

    async fn weather_request_coordinates(
        &self,
        connection_id: ConnectionId,
        source: &Callsign,
    ) -> Result<Option<(f64, f64)>, Effects> {
        let state = self.state.read().await;
        let Some(session) = state.sessions.get(&connection_id) else {
            return Err(Effects::default());
        };
        if session.phase != SessionPhase::Active {
            return Err(Effects::default());
        }
        if session.callsign() != Some(source) {
            return Err(Self::error_effect(
                connection_id,
                session.callsign().cloned(),
                ErrorCode::InvalidSource,
                source.to_string(),
            ));
        }
        Ok(session.position.as_ref().map(Position::coordinates))
    }

    fn parsed_weather_effect(
        connection_id: ConnectionId,
        source: Callsign,
        station: String,
        profile: Option<WeatherProfile>,
    ) -> Effects {
        let Some(profile) = profile else {
            return Self::no_weather(connection_id, source, station);
        };
        if let Err(error) = profile.validate() {
            tracing::warn!(
                %connection_id,
                %station,
                error = %error,
                "Weather provider returned an invalid parsed profile"
            );
            return Self::no_weather(connection_id, source, station);
        }
        Self::send(
            vec![connection_id],
            Event::WeatherProfile {
                destination: source,
                station,
                profile,
            },
        )
    }

    fn raw_weather_effect(
        connection_id: ConnectionId,
        source: Callsign,
        station: String,
        report: Option<String>,
    ) -> Effects {
        let Some(report) = report else {
            return Self::no_weather(connection_id, source, station);
        };
        if report.bytes().any(|byte| matches!(byte, b'\r' | b'\n')) {
            tracing::warn!(
                %connection_id,
                %station,
                "Weather provider returned a delimited raw report"
            );
            return Self::no_weather(connection_id, source, station);
        }
        Self::send(
            vec![connection_id],
            Event::WeatherReport {
                source: "server".to_string(),
                destination: source,
                station,
                report,
            },
        )
    }

    fn no_weather(connection_id: ConnectionId, source: Callsign, station: String) -> Effects {
        Self::error_effect(connection_id, Some(source), ErrorCode::NoWeather, station)
    }
}
