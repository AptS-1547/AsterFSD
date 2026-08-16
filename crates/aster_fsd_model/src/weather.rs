use crate::ModelError;
use serde::{Deserialize, Serialize};

/// One temperature layer in the classic FSD parsed-weather profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemperatureLayer {
    pub ceiling: i32,
    pub temperature: i32,
}

/// One wind layer in the classic FSD parsed-weather profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindLayer {
    pub ceiling: i32,
    pub floor: i32,
    pub direction: i32,
    pub speed: i32,
    pub gusting: i32,
    pub turbulence: i32,
}

/// One cloud or thunderstorm layer in the classic FSD parsed-weather profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudLayer {
    pub ceiling: i32,
    pub floor: i32,
    pub coverage: i32,
    pub icing: i32,
    pub turbulence: i32,
}

/// Fixed-shape parsed weather returned by the original C FSD as three frames.
///
/// The C wire contract always contains four temperature layers, four wind
/// layers, two ordinary cloud layers and one thunderstorm layer. Keeping those
/// counts in the type prevents a backend from silently emitting a truncated
/// `#TD`, `#WD` or `#CD` packet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherProfile {
    pub temperatures: [TemperatureLayer; 4],
    pub winds: [WindLayer; 4],
    pub clouds: [CloudLayer; 2],
    pub thunderstorm: CloudLayer,
    pub barometer: i32,
    pub visibility: f64,
}

impl WeatherProfile {
    /// Validates values whose textual representation has a protocol-level
    /// invariant independent of the upstream weather source.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::InvalidField`] when visibility is negative or
    /// non-finite. Integer layer values deliberately retain the original C FSD
    /// domain, including `-1` sentinel ceilings and floors.
    pub fn validate(&self) -> Result<(), ModelError> {
        if !self.visibility.is_finite() || self.visibility < 0.0 {
            return Err(ModelError::InvalidField {
                field: "weather visibility",
                reason: "must be finite and non-negative",
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_profile_validation_preserves_c_sentinels_and_rejects_invalid_visibility() {
        let mut profile = WeatherProfile {
            temperatures: [
                TemperatureLayer {
                    ceiling: 100,
                    temperature: 15,
                },
                TemperatureLayer {
                    ceiling: 10_000,
                    temperature: -5,
                },
                TemperatureLayer {
                    ceiling: 18_000,
                    temperature: -21,
                },
                TemperatureLayer {
                    ceiling: 35_000,
                    temperature: -51,
                },
            ],
            winds: [WindLayer {
                ceiling: -1,
                floor: -1,
                direction: 0,
                speed: 0,
                gusting: 0,
                turbulence: 0,
            }; 4],
            clouds: [CloudLayer {
                ceiling: -1,
                floor: -1,
                coverage: 0,
                icing: 0,
                turbulence: 0,
            }; 2],
            thunderstorm: CloudLayer {
                ceiling: -1,
                floor: -1,
                coverage: 0,
                icing: 0,
                turbulence: 0,
            },
            barometer: 2_950,
            visibility: 15.0,
        };
        assert!(profile.validate().is_ok());
        profile.visibility = -0.01;
        assert!(profile.validate().is_err());
        profile.visibility = f64::INFINITY;
        assert!(profile.validate().is_err());
    }
}
