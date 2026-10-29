use api::Availability;
use application::BuildInfo;
use application::DbContext;
use application::bootstrap_first_admin::MissingFirstAdmin;
use application::ports::Clock;
use application::ports::SendEmail;
use infrastructure::AppState;
use infrastructure::persistence;
use infrastructure::telemetry;
use ohno::AppError;
use ohno::ErrorExt;
use ohno::app_err;
use settings::AppSettings;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use topcoat::router::Router;

const DRAIN_PERIOD: Duration = Duration::from_secs(5);

pub struct Server {
    listener: TcpListener,
    router: Router,
    availability: Availability,
    state: AppState,
}

pub async fn start(
    settings: AppSettings,
    clock: Clock,
    send_email: SendEmail,
) -> Result<Server, AppError> {
    let db = persistence::connect(&settings.database).await?;
    persistence::migrate(&db).await?;
    let state = AppState::builder()
        .build_info(|_| BuildInfo {
            name: env!("CARGO_PKG_NAME"),
            version: env!("CARGO_PKG_VERSION"),
            git_branch: env!("GIT_BRANCH"),
            git_commit_id: env!("GIT_COMMIT"),
            git_commit_time: env!("GIT_COMMIT_TIME"),
        })
        .clock(|_| clock.clone())
        .db_context(|_| DbContext::new(db.clone(), settings.database.location()))
        .export_telemetry_try(|_| telemetry::export_telemetry(&settings.otlp))?
        .send_email(|_| send_email.clone())
        .build();
    application::bootstrap_first_admin::call(
        &state,
        settings.admin.username,
        settings.admin.password,
    )
    .await
    .map_err(|error| {
        if error.find_source::<MissingFirstAdmin>().is_some() {
            app_err!("no admin exists, so set ADMIN__USERNAME and ADMIN__PASSWORD to create the first one")
        } else {
            AppError::new(error)
        }
    })?;
    workers::start(&state).await?;
    let availability = Availability::default();
    let router = api::router(
        admin::register(Router::builder()),
        state.clone(),
        availability.clone(),
        std::path::absolute(&settings.web.dir)?,
    );
    let listener = TcpListener::bind(("0.0.0.0", settings.http.port)).await?;
    Ok(Server {
        listener,
        router,
        availability,
        state,
    })
}

impl Server {
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub async fn serve(self, shutdown: impl Future<Output = ()>) -> Result<(), AppError> {
        let availability = self.availability;
        let stop_workers = CancellationToken::new();
        let workers = tokio::spawn(workers::serve(self.state, stop_workers.clone()));
        availability.accept_traffic();
        tracing::info!(
            server.port = self.listener.local_addr()?.port(),
            "accepting traffic"
        );
        topcoat::serve_until(self.listener, self.router, async {
            shutdown.await;
            tracing::info!("shutdown started, refusing traffic");
            stop_workers.cancel();
            availability.refuse_traffic();
            tokio::time::sleep(DRAIN_PERIOD).await;
        })
        .await?;
        workers.await?;
        Ok(())
    }
}
