use application::DbLocation;
use serde::Deserialize;
use std::time::Duration;
use url::Url;

const POOL_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Deserialize)]
pub struct DatabaseSettings {
    pub url: Url,
}

impl DatabaseSettings {
    pub fn location(&self) -> DbLocation {
        DbLocation {
            database: self.url.path().trim_start_matches('/').to_owned(),
            address: self.url.host_str().unwrap_or_default().to_owned(),
            port: self.url.port().unwrap_or(5432),
        }
    }
}

#[ohno::error]
#[display("failed to connect to the database")]
#[from(toasty::Error)]
pub struct ConnectError;

#[ohno::error]
#[display("failed to apply the database migrations")]
#[from(toasty::Error)]
pub struct MigrateError;

pub async fn connect(settings: &DatabaseSettings) -> Result<toasty::Db, ConnectError> {
    let driver = toasty_driver_postgresql::PostgreSQL::new(settings.url.as_str())?;
    Ok(toasty::Db::builder()
        .models(application::models())
        .pool_create_timeout(Some(POOL_TIMEOUT))
        .pool_wait_timeout(Some(POOL_TIMEOUT))
        .build(driver)
        .await?)
}

pub async fn migrate(db: &toasty::Db) -> Result<(), MigrateError> {
    toasty::embed_migrations!().apply(db).await?;
    Ok(())
}
