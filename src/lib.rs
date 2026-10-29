use api::Availability;
use application::BuildInfo;
use application::DbContext;
use application::DbServer;
use infrastructure::AppState;
use infrastructure::persistence;
use infrastructure::telemetry;
use ohno::AppError;
use settings::AppSettings;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use topcoat::router::Router;
use url::Url;

const DRAIN_PERIOD: Duration = Duration::from_secs(5);

pub struct Server {
    listener: TcpListener,
    router: Router,
    availability: Availability,
}

pub async fn start(settings: AppSettings) -> Result<Server, AppError> {
    let db = persistence::connect(&settings.database).await?;
    persistence::migrate(&db).await?;
    let database_url = Url::parse(&settings.database.url)?;
    let server = DbServer {
        database: database_url.path().trim_start_matches('/').to_owned(),
        address: database_url.host_str().unwrap_or_default().to_owned(),
        port: database_url.port().unwrap_or(5432),
    };
    let state = AppState::builder()
        .build_info(|_| BuildInfo {
            name: env!("CARGO_PKG_NAME"),
            version: env!("CARGO_PKG_VERSION"),
            git_branch: env!("GIT_BRANCH"),
            git_commit_id: env!("GIT_COMMIT"),
            git_commit_time: env!("GIT_COMMIT_TIME"),
        })
        .db_context(|_| DbContext::new(db.clone(), server.clone()))
        .export_telemetry_try(|_| telemetry::export_telemetry(&settings.otlp))?
        .build();
    let availability = Availability::default();
    let router = api::router(
        admin::register(Router::builder()),
        state,
        availability.clone(),
        std::path::absolute(&settings.web.dir)?,
    );
    let listener = TcpListener::bind(("0.0.0.0", settings.http.port)).await?;
    Ok(Server {
        listener,
        router,
        availability,
    })
}

impl Server {
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub async fn serve(self, shutdown: impl Future<Output = ()>) -> Result<(), AppError> {
        let availability = self.availability;
        availability.accept_traffic();
        tracing::info!(
            server.port = self.listener.local_addr()?.port(),
            "accepting traffic"
        );
        topcoat::serve_until(self.listener, self.router, async {
            shutdown.await;
            tracing::info!("shutdown started, refusing traffic");
            availability.refuse_traffic();
            tokio::time::sleep(DRAIN_PERIOD).await;
        })
        .await?;
        Ok(())
    }
}
