use aster_fsd_model::WeatherProfile;
use async_trait::async_trait;
use thiserror::Error;

/// Coordinates and request metadata supplied to an external weather source.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherLookup {
    pub station: String,
    pub coordinates: Option<(f64, f64)>,
}

/// Raw and parsed forms returned by one weather-source lookup.
///
/// A source may provide either representation. The core selects the form the
/// client requested and returns classic `$ER009` when that form is absent.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherObservation {
    pub raw_metar: Option<String>,
    pub profile: Option<WeatherProfile>,
}

/// Weather-source failure kept behind the core port boundary.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct WeatherProviderError {
    message: String,
}

impl WeatherProviderError {
    /// Creates a provider error suitable for structured server diagnostics.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Asynchronous port used by the network core to resolve weather data.
#[async_trait]
pub trait WeatherProvider: Send + Sync {
    /// Resolves raw and/or parsed weather for one station.
    ///
    /// # Errors
    ///
    /// Returns [`WeatherProviderError`] when the upstream source fails. A
    /// successful lookup with no matching station returns `Ok(None)`.
    async fn lookup(
        &self,
        request: &WeatherLookup,
    ) -> Result<Option<WeatherObservation>, WeatherProviderError>;
}

/// Explicit provider used when no weather adapter is configured.
#[derive(Debug, Default)]
pub struct UnavailableWeatherProvider;

#[async_trait]
impl WeatherProvider for UnavailableWeatherProvider {
    async fn lookup(
        &self,
        _request: &WeatherLookup,
    ) -> Result<Option<WeatherObservation>, WeatherProviderError> {
        Ok(None)
    }
}
