use config::Config;
use config::ConfigError;
use config::Environment;
use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;

pub use persistence::DatabaseSettings;
pub use radarr::RadarrSettings;
pub use sonarr::SonarrSettings;
pub use telemetry::OtlpSettings;

#[derive(Clone, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub http: HttpSettings,
    pub database: DatabaseSettings,
    #[serde(default)]
    pub web: WebSettings,
    #[serde(flatten)]
    pub otlp: OtlpSettings,
    #[serde(default)]
    pub radarr: RadarrSettings,
    #[serde(default)]
    pub sonarr: SonarrSettings,
    #[serde(default)]
    pub library_sync: LibrarySyncSettings,
}

impl AppSettings {
    pub fn load() -> Result<Self, ConfigError> {
        Config::builder()
            .add_source(Environment::default().separator("__"))
            .build()?
            .try_deserialize()
    }
}

#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct HttpSettings {
    pub port: u16,
}

impl Default for HttpSettings {
    fn default() -> Self {
        Self { port: 8080 }
    }
}

#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct WebSettings {
    pub dir: PathBuf,
}

impl Default for WebSettings {
    fn default() -> Self {
        Self {
            dir: PathBuf::from("presentation/web/dist"),
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct LibrarySyncSettings {
    #[serde(with = "jiff::fmt::serde::unsigned_duration::friendly::compact::required")]
    pub reconcile_interval: Duration,
}

impl Default for LibrarySyncSettings {
    fn default() -> Self {
        Self {
            reconcile_interval: Duration::from_secs(3600),
        }
    }
}
