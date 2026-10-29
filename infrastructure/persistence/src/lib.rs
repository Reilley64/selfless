use serde::Deserialize;
use std::time::Duration;

#[derive(Clone, Deserialize)]
pub struct DatabaseSettings {
    pub url: String,
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
    let driver = toasty_driver_postgresql::PostgreSQL::new(&settings.url)?;
    Ok(toasty::Db::builder()
        .pool_create_timeout(Some(Duration::from_secs(1)))
        .pool_wait_timeout(Some(Duration::from_secs(1)))
        .build(driver)
        .await?)
}

pub async fn migrate(db: &toasty::Db) -> Result<(), MigrateError> {
    toasty::embed_migrations!().apply(db).await?;
    Ok(())
}
