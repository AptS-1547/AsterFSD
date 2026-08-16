use crate::{Callsign, ModelError};
use serde::{Deserialize, Serialize};

/// Authoritative pilot position decoded from a protocol adapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PilotPosition {
    pub callsign: Callsign,
    pub mode: char,
    pub squawk: String,
    pub rating: i32,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: i32,
    pub groundspeed: i32,
    pub pbh: u32,
    pub flags: i32,
}

/// Authoritative controller position decoded from a protocol adapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtcPosition {
    pub callsign: Callsign,
    pub frequency: i32,
    pub facility_type: i32,
    pub visual_range: i32,
    pub rating: i32,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: i32,
}

/// Position update for either a pilot or controller session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Position {
    Pilot(PilotPosition),
    Atc(AtcPosition),
}

impl Position {
    /// Returns the callsign whose authoritative position is being updated.
    #[must_use]
    pub fn callsign(&self) -> &Callsign {
        match self {
            Self::Pilot(position) => &position.callsign,
            Self::Atc(position) => &position.callsign,
        }
    }

    /// Returns latitude and longitude in decimal degrees.
    #[must_use]
    pub fn coordinates(&self) -> (f64, f64) {
        match self {
            Self::Pilot(position) => (position.latitude, position.longitude),
            Self::Atc(position) => (position.latitude, position.longitude),
        }
    }

    /// Returns the reported altitude in feet.
    #[must_use]
    pub fn altitude(&self) -> i32 {
        match self {
            Self::Pilot(position) => position.altitude,
            Self::Atc(position) => position.altitude,
        }
    }

    /// Validates protocol-independent position invariants.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::InvalidField`] for out-of-range coordinates,
    /// invalid pilot mode or squawk values, and negative ATC visual range.
    ///
    /// Classic FSD represents a transponder as an integer. Consequently,
    /// leading zeroes are optional on the wire: `0` and `0000` both represent
    /// the same valid code.
    pub fn validate(&self) -> Result<(), ModelError> {
        let (latitude, longitude) = self.coordinates();
        if !latitude.is_finite() || !(-90.0..=90.0).contains(&latitude) {
            return Err(ModelError::InvalidField {
                field: "latitude",
                reason: "must be finite and between -90 and 90",
            });
        }
        if !longitude.is_finite() || !(-180.0..=180.0).contains(&longitude) {
            return Err(ModelError::InvalidField {
                field: "longitude",
                reason: "must be finite and between -180 and 180",
            });
        }
        match self {
            Self::Pilot(position) => {
                if !matches!(position.mode, 'N' | 'S' | 'Y') {
                    return Err(ModelError::InvalidField {
                        field: "mode",
                        reason: "must be N, S, or Y",
                    });
                }
                if position.squawk.is_empty()
                    || position.squawk.len() > 4
                    || !position
                        .squawk
                        .bytes()
                        .all(|byte| matches!(byte, b'0'..=b'7'))
                {
                    return Err(ModelError::InvalidField {
                        field: "squawk",
                        reason: "must contain between one and four octal digits",
                    });
                }
            }
            Self::Atc(position) if position.visual_range < 0 => {
                return Err(ModelError::InvalidField {
                    field: "visual_range",
                    reason: "must be non-negative",
                });
            }
            Self::Atc(_) => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_validation_covers_numeric_and_protocol_boundaries() {
        let pilot = |latitude, longitude, squawk: &str, mode| {
            Position::Pilot(PilotPosition {
                callsign: Callsign::parse("ECP1").unwrap(),
                mode,
                squawk: squawk.to_string(),
                rating: 1,
                latitude,
                longitude,
                altitude: 0,
                groundspeed: 0,
                pbh: 0,
                flags: 0,
            })
        };

        assert!(pilot(-90.0, 180.0, "0000", 'N').validate().is_ok());
        assert!(pilot(0.0, 0.0, "0", 'N').validate().is_ok());
        assert!(pilot(0.0, 0.0, "0001", 'N').validate().is_ok());
        assert!(pilot(90.0, -180.0, "7777", 'Y').validate().is_ok());
        assert!(pilot(f64::NAN, 0.0, "1200", 'N').validate().is_err());
        assert!(pilot(90.000_001, 0.0, "1200", 'N').validate().is_err());
        assert!(pilot(0.0, -180.000_001, "1200", 'N').validate().is_err());
        assert!(pilot(0.0, 0.0, "1280", 'N').validate().is_err());
        assert!(pilot(0.0, 0.0, "", 'N').validate().is_err());
        assert!(pilot(0.0, 0.0, "10000", 'N').validate().is_err());
        assert!(pilot(0.0, 0.0, "1200", 'X').validate().is_err());

        let atc = Position::Atc(AtcPosition {
            callsign: Callsign::parse("CTR1").unwrap(),
            frequency: 199_998,
            facility_type: 5,
            visual_range: -1,
            rating: 5,
            latitude: 0.0,
            longitude: 0.0,
            altitude: 0,
        });
        assert!(atc.validate().is_err());
    }
}
