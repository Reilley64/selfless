use config::Config;
use config::ConfigError;
use config::Environment;
use serde::Deserialize;

pub use persistence::DatabaseSettings;

#[derive(Clone, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub http: HttpSettings,
    pub database: DatabaseSettings,
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
