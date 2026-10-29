use config::Config;
use config::ConfigError;
use config::Environment;
use serde::Deserialize;
use std::path::PathBuf;

pub use email::SmtpSettings;
pub use persistence::DatabaseSettings;
pub use telemetry::OtlpSettings;

#[derive(Clone, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub http: HttpSettings,
    pub database: DatabaseSettings,
    #[serde(default)]
    pub web: WebSettings,
    #[serde(default)]
    pub admin: AdminSettings,
    #[serde(flatten)]
    pub otlp: OtlpSettings,
    #[serde(default)]
    pub smtp: SmtpSettings,
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

#[derive(Clone, Default, Deserialize)]
pub struct AdminSettings {
    pub username: Option<String>,
    pub password: Option<String>,
}
