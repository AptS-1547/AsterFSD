use super::{
    AuthenticatedIdentity, Callsign, ClientType, Command, Position, VATSIM_PROTOCOL_REVISION,
    WeatherLookup, WeatherProfile, WeatherProvider,
};
use crate::{CoreConfig, Network, WeatherObservation, WeatherProviderError};
use aster_fsd_auth::{AllowAllAuthenticator, AuthError, Authenticator};
use aster_fsd_model::{
    AtcPosition, CloudLayer, FlightPlan, Identification, LoginCommand, PilotPosition,
    TemperatureLayer, WindLayer,
};
use async_trait::async_trait;
use std::sync::Arc;

include!("support.rs");

mod authentication;
mod lifecycle;
mod routing;
mod state;
mod weather;
