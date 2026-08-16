use crate::{Callsign, ModelError};
use serde::{Deserialize, Serialize};

/// Typed delivery destination understood by the network core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Destination {
    Server,
    Direct(Callsign),
    All,
    Atc,
    Pilots,
    Range(String),
}

impl Destination {
    /// Parses a classic destination token into a routing intent.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::EmptyDestination`] for an empty token and
    /// propagates callsign validation failures for direct destinations.
    pub fn parse(value: &str) -> Result<Self, ModelError> {
        if value.is_empty() {
            return Err(ModelError::EmptyDestination);
        }
        match value.to_ascii_uppercase().as_str() {
            "SERVER" => Ok(Self::Server),
            "*" => Ok(Self::All),
            "*A" => Ok(Self::Atc),
            "*P" => Ok(Self::Pilots),
            _ if value.starts_with('@') => Ok(Self::Range(value.to_string())),
            _ => Callsign::parse(value).map(Self::Direct),
        }
    }
}

/// Semantic query category shared across protocol encodings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryKind {
    RealName,
    FlightPlan,
    Capabilities,
    Atis,
    SystemInfo,
    AircraftConfiguration,
    Raw(String),
}

/// Direction of an ATC handoff transaction relayed by the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffKind {
    Request,
    Accept,
}

/// Typed legacy client-data exchange retained by classic FSD clients.
///
/// These variants have direct-recipient semantics but no authoritative server
/// state in the original C implementation. Keeping the kind typed prevents
/// classic command tokens from leaking into the shared core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientDataKind {
    SquawkBox,
    ProController,
    CommunicationRequest,
    CommunicationReply,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_destinations_are_typed() {
        assert_eq!(Destination::parse("*A").unwrap(), Destination::Atc);
        assert_eq!(Destination::parse("*P").unwrap(), Destination::Pilots);
        assert_eq!(
            Destination::parse("@94836").unwrap(),
            Destination::Range("@94836".to_string())
        );
    }
}
