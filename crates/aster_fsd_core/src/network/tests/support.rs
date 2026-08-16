#[derive(Debug, Clone, Copy)]
enum AuthenticationFailure {
    InvalidCredentials,
    Suspended,
}

#[derive(Debug)]
struct FailingAuthenticator(AuthenticationFailure);

#[async_trait]
impl Authenticator for FailingAuthenticator {
    async fn authorize_client(&self, _client_id: &str) -> Result<(), AuthError> {
        Ok(())
    }

    async fn authenticate(
        &self,
        _network_id: &str,
        _password: &str,
    ) -> Result<AuthenticatedIdentity, AuthError> {
        Err(match self.0 {
            AuthenticationFailure::InvalidCredentials => AuthError::InvalidCredentials,
            AuthenticationFailure::Suspended => AuthError::Suspended,
        })
    }
}

fn pilot_login(callsign: &str) -> Command {
    Command::Login(LoginCommand {
        callsign: Callsign::parse(callsign).unwrap(),
        client_type: ClientType::Pilot,
        network_id: callsign.to_string(),
        password: "secret".to_string(),
        requested_rating: 1,
        protocol_revision: 9,
        real_name: callsign.to_string(),
        simulator_type: Some(2),
    })
}

fn atc_login(callsign: &str, rating: i32) -> Command {
    Command::Login(LoginCommand {
        callsign: Callsign::parse(callsign).unwrap(),
        client_type: ClientType::Atc,
        network_id: callsign.to_string(),
        password: "secret".to_string(),
        requested_rating: rating,
        protocol_revision: 9,
        real_name: callsign.to_string(),
        simulator_type: None,
    })
}

fn vatsim_identification(callsign: &str, network_id: Option<&str>) -> Command {
    Command::Identify(Identification {
        callsign: Callsign::parse(callsign).unwrap(),
        client_id: "48e2".to_string(),
        client_name: "swift".to_string(),
        network_id: network_id.map(str::to_string),
    })
}

fn vatsim_pilot_login(callsign: &str, network_id: &str) -> Command {
    let Command::Login(mut login) = pilot_login(callsign) else {
        panic!("expected pilot login command");
    };
    login.network_id = network_id.to_string();
    login.protocol_revision = VATSIM_PROTOCOL_REVISION;
    Command::Login(login)
}

fn vatsim_atc_login(callsign: &str, network_id: &str) -> Command {
    let Command::Login(mut login) = atc_login(callsign, 5) else {
        panic!("expected ATC login command");
    };
    login.network_id = network_id.to_string();
    login.protocol_revision = VATSIM_PROTOCOL_REVISION;
    Command::Login(login)
}

fn pilot_position(callsign: &str, latitude: f64, longitude: f64, altitude: i32) -> Command {
    Command::Position(Position::Pilot(PilotPosition {
        callsign: Callsign::parse(callsign).unwrap(),
        mode: 'N',
        squawk: "1200".to_string(),
        rating: 1,
        latitude,
        longitude,
        altitude,
        groundspeed: 200,
        pbh: 0,
        flags: 0,
    }))
}

fn atc_position(
    callsign: &str,
    latitude: f64,
    longitude: f64,
    facility_type: i32,
    visual_range: i32,
) -> Command {
    Command::Position(Position::Atc(AtcPosition {
        callsign: Callsign::parse(callsign).unwrap(),
        frequency: 199_998,
        facility_type,
        visual_range,
        rating: 5,
        latitude,
        longitude,
        altitude: 0,
    }))
}

fn flight_plan(callsign: &str) -> FlightPlan {
    FlightPlan {
        callsign: Callsign::parse(callsign).unwrap(),
        flight_rules: 'I',
        aircraft: "B738".to_string(),
        cruise_speed: 450,
        departure: "ZSPD".to_string(),
        estimated_departure: 1200,
        actual_departure: 1205,
        cruise_altitude: "FL350".to_string(),
        destination: "ZBAA".to_string(),
        hours_enroute: 2,
        minutes_enroute: 0,
        hours_fuel: 4,
        minutes_fuel: 0,
        alternate: "ZSNJ".to_string(),
        remarks: "RMK".to_string(),
        route: "DCT PIKAS DCT".to_string(),
    }
}

fn network() -> Network {
    Network::new(CoreConfig::default(), Arc::new(AllowAllAuthenticator))
}

fn weather_profile() -> WeatherProfile {
    WeatherProfile {
        temperatures: [TemperatureLayer {
            ceiling: 100,
            temperature: 15,
        }; 4],
        winds: [WindLayer {
            ceiling: 2_500,
            floor: 0,
            direction: 180,
            speed: 12,
            gusting: 0,
            turbulence: 1,
        }; 4],
        clouds: [CloudLayer {
            ceiling: 5_000,
            floor: 3_000,
            coverage: 4,
            icing: 0,
            turbulence: 1,
        }; 2],
        thunderstorm: CloudLayer {
            ceiling: 35_000,
            floor: 20_000,
            coverage: 1,
            icing: 2,
            turbulence: 3,
        },
        barometer: 2_992,
        visibility: 12.5,
    }
}

#[derive(Clone)]
struct FixtureWeatherProvider {
    result: Result<Option<WeatherObservation>, WeatherProviderError>,
}

#[async_trait]
impl WeatherProvider for FixtureWeatherProvider {
    async fn lookup(
        &self,
        _request: &WeatherLookup,
    ) -> Result<Option<WeatherObservation>, WeatherProviderError> {
        self.result.clone()
    }
}

fn weather_network(result: Result<Option<WeatherObservation>, WeatherProviderError>) -> Network {
    Network::with_weather_provider(
        CoreConfig::default(),
        Arc::new(AllowAllAuthenticator),
        Arc::new(FixtureWeatherProvider { result }),
    )
}
