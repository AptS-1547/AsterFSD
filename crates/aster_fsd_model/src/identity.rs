use crate::ModelError;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;

/// A validated, normalized and case-insensitive FSD callsign.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Callsign(String);

impl Callsign {
    /// Parses and normalizes a callsign to uppercase ASCII.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::InvalidCallsignLength`] when the value is not 2
    /// through 12 ASCII bytes, or [`ModelError::InvalidCallsignCharacter`]
    /// when it contains a character reserved by classic FSD framing.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ModelError> {
        let value = value.as_ref();
        if !(2..=12).contains(&value.len()) || !value.is_ascii() {
            return Err(ModelError::InvalidCallsignLength);
        }
        if value.bytes().any(|byte| {
            matches!(
                byte,
                b'!' | b'@' | b'#' | b'$' | b'%' | b'*' | b':' | b'&' | b' ' | b'\t'
            )
        }) {
            return Err(ModelError::InvalidCallsignCharacter);
        }
        Ok(Self(value.to_ascii_uppercase()))
    }

    /// Returns the normalized callsign without allocating.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Callsign {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Serialize for Callsign {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Callsign {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(de::Error::custom)
    }
}

/// Client role used by authorization, routing and recipient filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientType {
    Pilot,
    Atc,
    Observer,
}

/// Classic FSD controller certification stored as a stable domain value.
///
/// The database representation is deliberately a semantic string rather than
/// the classic wire integer. Wire values remain an adapter concern and are
/// converted explicitly through [`Self::wire_value`] and [`Self::try_from`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum, Serialize, Deserialize,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum AtcRating {
    /// Certification is suspended and cannot be used for login.
    #[sea_orm(string_value = "suspended")]
    Suspended,
    /// Observer or pilot-only access.
    #[sea_orm(string_value = "observer")]
    Observer,
    /// First student-controller level.
    #[sea_orm(string_value = "student_1")]
    #[serde(rename = "student_1")]
    Student1,
    /// Second student-controller level.
    #[sea_orm(string_value = "student_2")]
    #[serde(rename = "student_2")]
    Student2,
    /// Third student-controller level.
    #[sea_orm(string_value = "student_3")]
    #[serde(rename = "student_3")]
    Student3,
    /// First controller level.
    #[sea_orm(string_value = "controller_1")]
    #[serde(rename = "controller_1")]
    Controller1,
    /// Second controller level.
    #[sea_orm(string_value = "controller_2")]
    #[serde(rename = "controller_2")]
    Controller2,
    /// Third controller level.
    #[sea_orm(string_value = "controller_3")]
    #[serde(rename = "controller_3")]
    Controller3,
    /// First instructor level.
    #[sea_orm(string_value = "instructor_1")]
    #[serde(rename = "instructor_1")]
    Instructor1,
    /// Second instructor level.
    #[sea_orm(string_value = "instructor_2")]
    #[serde(rename = "instructor_2")]
    Instructor2,
    /// Third instructor level.
    #[sea_orm(string_value = "instructor_3")]
    #[serde(rename = "instructor_3")]
    Instructor3,
    /// Network supervisor level.
    #[sea_orm(string_value = "supervisor")]
    Supervisor,
    /// Network administrator level.
    #[sea_orm(string_value = "administrator")]
    Administrator,
}

impl AtcRating {
    /// Returns the classic FSD certification-level integer for this rating.
    #[must_use]
    pub const fn wire_value(self) -> i32 {
        match self {
            Self::Suspended => 0,
            Self::Observer => 1,
            Self::Student1 => 2,
            Self::Student2 => 3,
            Self::Student3 => 4,
            Self::Controller1 => 5,
            Self::Controller2 => 6,
            Self::Controller3 => 7,
            Self::Instructor1 => 8,
            Self::Instructor2 => 9,
            Self::Instructor3 => 10,
            Self::Supervisor => 11,
            Self::Administrator => 12,
        }
    }

    /// Returns whether this identity may log in at the requested ATC wire level.
    #[must_use]
    pub const fn allows_wire_value(self, requested_rating: i32) -> bool {
        !matches!(self, Self::Suspended)
            && matches!(requested_rating, 1..=12)
            && requested_rating <= self.wire_value()
    }
}

impl TryFrom<i32> for AtcRating {
    type Error = ModelError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Suspended),
            1 => Ok(Self::Observer),
            2 => Ok(Self::Student1),
            3 => Ok(Self::Student2),
            4 => Ok(Self::Student3),
            5 => Ok(Self::Controller1),
            6 => Ok(Self::Controller2),
            7 => Ok(Self::Controller3),
            8 => Ok(Self::Instructor1),
            9 => Ok(Self::Instructor2),
            10 => Ok(Self::Instructor3),
            11 => Ok(Self::Supervisor),
            12 => Ok(Self::Administrator),
            _ => Err(ModelError::InvalidField {
                field: "ATC rating",
                reason: "must be a recognized classic FSD certification level",
            }),
        }
    }
}

/// VATSIM pilot certification stored as a stable domain value.
///
/// Pilot ratings are cumulative capability values, not a contiguous integer
/// sequence. They must therefore never share an integer type or range check
/// with [`AtcRating`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum, Serialize, Deserialize,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum PilotRating {
    /// No pilot certification; this value cannot be used for login.
    #[sea_orm(string_value = "unrated")]
    Unrated,
    /// Private pilot licence.
    #[sea_orm(string_value = "private_pilot_license")]
    PrivatePilotLicense,
    /// Instrument rating.
    #[sea_orm(string_value = "instrument_rating")]
    InstrumentRating,
    /// Commercial multi-engine licence.
    #[sea_orm(string_value = "commercial_multi_engine_license")]
    CommercialMultiEngineLicense,
    /// Airline transport pilot licence.
    #[sea_orm(string_value = "airline_transport_pilot_license")]
    AirlineTransportPilotLicense,
    /// Flight instructor certification.
    #[sea_orm(string_value = "flight_instructor")]
    FlightInstructor,
    /// Flight examiner certification.
    #[sea_orm(string_value = "flight_examiner")]
    FlightExaminer,
}

impl PilotRating {
    /// Returns the VATSIM cumulative pilot-rating integer for this rating.
    #[must_use]
    pub const fn wire_value(self) -> i32 {
        match self {
            Self::Unrated => 0,
            Self::PrivatePilotLicense => 1,
            Self::InstrumentRating => 3,
            Self::CommercialMultiEngineLicense => 7,
            Self::AirlineTransportPilotLicense => 15,
            Self::FlightInstructor => 31,
            Self::FlightExaminer => 63,
        }
    }

    /// Returns whether this identity may log in at the requested pilot wire level.
    #[must_use]
    pub const fn allows_wire_value(self, requested_rating: i32) -> bool {
        !matches!(self, Self::Unrated)
            && matches!(requested_rating, 1 | 3 | 7 | 15 | 31 | 63)
            && requested_rating <= self.wire_value()
    }
}

impl TryFrom<i32> for PilotRating {
    type Error = ModelError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Unrated),
            1 => Ok(Self::PrivatePilotLicense),
            3 => Ok(Self::InstrumentRating),
            7 => Ok(Self::CommercialMultiEngineLicense),
            15 => Ok(Self::AirlineTransportPilotLicense),
            31 => Ok(Self::FlightInstructor),
            63 => Ok(Self::FlightExaminer),
            _ => Err(ModelError::InvalidField {
                field: "pilot rating",
                reason: "must be a recognized VATSIM cumulative certification level",
            }),
        }
    }
}

/// Identity returned by the authentication boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticatedIdentity {
    pub network_id: String,
    pub real_name: String,
    pub atc_rating: AtcRating,
    pub pilot_rating: PilotRating,
    pub suspended: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atc_ratings_have_stable_wire_and_serde_representations() {
        let ratings = [
            (AtcRating::Suspended, 0, "suspended"),
            (AtcRating::Observer, 1, "observer"),
            (AtcRating::Student1, 2, "student_1"),
            (AtcRating::Student2, 3, "student_2"),
            (AtcRating::Student3, 4, "student_3"),
            (AtcRating::Controller1, 5, "controller_1"),
            (AtcRating::Controller2, 6, "controller_2"),
            (AtcRating::Controller3, 7, "controller_3"),
            (AtcRating::Instructor1, 8, "instructor_1"),
            (AtcRating::Instructor2, 9, "instructor_2"),
            (AtcRating::Instructor3, 10, "instructor_3"),
            (AtcRating::Supervisor, 11, "supervisor"),
            (AtcRating::Administrator, 12, "administrator"),
        ];
        for (rating, wire_value, serialized) in ratings {
            assert_eq!(rating.wire_value(), wire_value);
            assert_eq!(AtcRating::try_from(wire_value).unwrap(), rating);
            assert_eq!(
                serde_json::to_string(&rating).unwrap(),
                format!("\"{serialized}\"")
            );
        }
        for invalid in [-1, 13, i32::MAX] {
            assert!(AtcRating::try_from(invalid).is_err());
        }
        assert!(AtcRating::Administrator.allows_wire_value(12));
        assert!(!AtcRating::Observer.allows_wire_value(2));
        assert!(!AtcRating::Suspended.allows_wire_value(0));
    }

    #[test]
    fn pilot_ratings_reject_gaps_in_the_cumulative_wire_encoding() {
        let ratings = [
            (PilotRating::Unrated, 0, "unrated"),
            (PilotRating::PrivatePilotLicense, 1, "private_pilot_license"),
            (PilotRating::InstrumentRating, 3, "instrument_rating"),
            (
                PilotRating::CommercialMultiEngineLicense,
                7,
                "commercial_multi_engine_license",
            ),
            (
                PilotRating::AirlineTransportPilotLicense,
                15,
                "airline_transport_pilot_license",
            ),
            (PilotRating::FlightInstructor, 31, "flight_instructor"),
            (PilotRating::FlightExaminer, 63, "flight_examiner"),
        ];
        for (rating, wire_value, serialized) in ratings {
            assert_eq!(rating.wire_value(), wire_value);
            assert_eq!(PilotRating::try_from(wire_value).unwrap(), rating);
            assert_eq!(
                serde_json::to_string(&rating).unwrap(),
                format!("\"{serialized}\"")
            );
        }
        for invalid in [-1, 2, 4, 6, 8, 16, 32, 64, i32::MAX] {
            assert!(PilotRating::try_from(invalid).is_err());
        }
        assert!(PilotRating::FlightExaminer.allows_wire_value(63));
        assert!(PilotRating::InstrumentRating.allows_wire_value(1));
        assert!(!PilotRating::InstrumentRating.allows_wire_value(7));
        assert!(!PilotRating::FlightExaminer.allows_wire_value(2));
        assert!(!PilotRating::Unrated.allows_wire_value(0));
    }

    #[test]
    fn callsigns_are_case_insensitive_and_support_classic_length() {
        assert_eq!(Callsign::parse("ecp4143").unwrap().as_str(), "ECP4143");
        assert!(Callsign::parse("AB").is_ok());
        assert!(Callsign::parse("ABCDEFGHIJKL").is_ok());
        assert!(Callsign::parse("A").is_err());
        assert!(Callsign::parse("ABC DEF").is_err());
    }
}
