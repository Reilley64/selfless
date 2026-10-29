mod telemetry;

use ohno::AppError;
use ohno::IntoAppError;
use settings::AppSettings;
use tokio::signal::unix::SignalKind;
use tokio::signal::unix::signal;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let settings = AppSettings::load()?;
    let mut terminate =
        signal(SignalKind::terminate()).into_app_err("failed to install the SIGTERM handler")?;
    let telemetry = telemetry::init()?;
    let result = async {
        selfless::start(settings, infrastructure::system_clock())
            .await?
            .serve(async move {
                tokio::select! {
                    _ = terminate.recv() => {}
                    _ = tokio::signal::ctrl_c() => {}
                }
            })
            .await
    }
    .await;
    telemetry.shutdown();
    result
}
