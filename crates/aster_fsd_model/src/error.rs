use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Domain validation failures produced before a command enters the core.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ModelError {
    #[error("callsign must contain between 2 and 12 ASCII bytes")]
    InvalidCallsignLength,
    #[error("callsign contains a reserved character")]
    InvalidCallsignCharacter,
    #[error("destination is empty")]
    EmptyDestination,
    #[error("field {field} is invalid: {reason}")]
    InvalidField {
        field: &'static str,
        reason: &'static str,
    },
}

/// Classic FSD error codes retained as cross-backend semantic errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum ErrorCode {
    NoError = 0,
    CallsignInUse = 1,
    InvalidCallsign = 2,
    AlreadyRegistered = 3,
    Syntax = 4,
    InvalidSource = 5,
    InvalidCredentials = 6,
    NoSuchCallsign = 7,
    NoFlightPlan = 8,
    NoWeather = 9,
    InvalidProtocolRevision = 10,
    RequestedLevelTooHigh = 11,
    ServerFull = 12,
    Suspended = 13,
    UnauthorizedClient = 16,
}

impl ErrorCode {
    /// Returns the canonical human-readable classic FSD description.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::NoError => "No error",
            Self::CallsignInUse => "Callsign in use",
            Self::InvalidCallsign => "Invalid callsign",
            Self::AlreadyRegistered => "Already registerd",
            Self::Syntax => "Syntax error",
            Self::InvalidSource => "Invalid source callsign",
            Self::InvalidCredentials => "Invalid CID/password",
            Self::NoSuchCallsign => "No such callsign",
            Self::NoFlightPlan => "No flightplan",
            Self::NoWeather => "No such weather profile",
            Self::InvalidProtocolRevision => "Invalid protocol revision",
            Self::RequestedLevelTooHigh => "Requested level too high",
            Self::ServerFull => "Too many clients connected",
            Self::Suspended => "CID/PID was suspended",
            Self::UnauthorizedClient => "Unauthorized client software",
        }
    }
}
