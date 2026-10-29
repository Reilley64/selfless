use infrastructure::persistence;
use ohno::AppError;
use settings::AppSettings;
use toasty_cli::Config;
use toasty_cli::ToastyCli;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let config = Config::load().map_err(AppError::new)?;
    let settings = AppSettings::load()?;
    let db = persistence::connect(&settings.database).await?;
    ToastyCli::with_config(db, config)
        .parse_and_run()
        .await
        .map_err(AppError::new)
}
