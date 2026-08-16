use aster_fsd_codec::RawPacketKind;
use aster_fsd_model::{Callsign, WeatherProfile};
use aster_fsd_protocol::{ProtocolError, ProtocolErrorKind, WireFrame};

use super::command_frame;

pub(super) fn encode_profile(
    destination: &Callsign,
    profile: &WeatherProfile,
) -> Result<Vec<WireFrame>, ProtocolError> {
    profile
        .validate()
        .map_err(|error| ProtocolError::new(ProtocolErrorKind::Encoding, error.to_string()))?;

    let mut temperature_fields = Vec::with_capacity(9);
    for layer in profile.temperatures {
        temperature_fields.push(layer.ceiling.to_string());
        temperature_fields.push(layer.temperature.to_string());
    }
    temperature_fields.push(profile.barometer.to_string());

    let mut wind_fields = Vec::with_capacity(24);
    for layer in profile.winds {
        wind_fields.push(layer.ceiling.to_string());
        wind_fields.push(layer.floor.to_string());
        wind_fields.push(layer.direction.to_string());
        wind_fields.push(layer.speed.to_string());
        wind_fields.push(layer.gusting.to_string());
        wind_fields.push(layer.turbulence.to_string());
    }

    let mut cloud_fields = Vec::with_capacity(16);
    for layer in profile
        .clouds
        .iter()
        .chain(std::iter::once(&profile.thunderstorm))
    {
        cloud_fields.push(layer.ceiling.to_string());
        cloud_fields.push(layer.floor.to_string());
        cloud_fields.push(layer.coverage.to_string());
        cloud_fields.push(layer.icing.to_string());
        cloud_fields.push(layer.turbulence.to_string());
    }
    cloud_fields.push(format!("{:.2}", profile.visibility));

    let destination = destination.to_string();
    Ok(vec![
        command_frame(
            RawPacketKind::Client,
            "TD",
            "server".to_string(),
            destination.clone(),
            temperature_fields,
        )?,
        command_frame(
            RawPacketKind::Client,
            "WD",
            "server".to_string(),
            destination.clone(),
            wind_fields,
        )?,
        command_frame(
            RawPacketKind::Client,
            "CD",
            "server".to_string(),
            destination,
            cloud_fields,
        )?,
    ])
}
