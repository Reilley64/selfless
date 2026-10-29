use api::Availability;
use application::BuildInfo;
use application::DbContext;
use application::InboxWakeup;
use application::Reconciles;
use application::WebhookPasswords;
use application::ports::Clock;
use infrastructure::AppState;
use infrastructure::media_files;
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

const DRAIN_PERIOD: Duration = Duration::from_secs(5);

pub struct Server {
    listener: TcpListener,
    router: Router,
    availability: Availability,
    state: AppState,
    reconcile_interval: Duration,
}

pub async fn start(settings: AppSettings, clock: Clock) -> Result<Server, AppError> {
    let get_radarr_movie = settings.radarr.get_movie()?;
    let webhook_passwords = WebhookPasswords {
        radarr: settings.radarr.webhook_password()?,
        sonarr: settings.sonarr.webhook_password()?,
    };
    let get_sonarr_show = settings.sonarr.get_show()?;
    let list_radarr_movies = settings.radarr.list_movies()?;
    let list_sonarr_series = settings.sonarr.list_series()?;
    let get_sonarr_episodes = settings.sonarr.get_episodes()?;
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
        .get_radarr_movie(|_| get_radarr_movie.clone())
        .get_sonarr_episodes(|_| get_sonarr_episodes.clone())
        .get_sonarr_show(|_| get_sonarr_show.clone())
        .inbox_wakeup(|_| InboxWakeup::default())
        .list_radarr_movies(|_| list_radarr_movies.clone())
        .list_sonarr_series(|_| list_sonarr_series.clone())
        .read_movie_folder(|_| media_files::read_movie_folder())
        .read_movie_nfo(|_| media_files::read_movie_nfo())
        .reconciles(|_| Reconciles::default())
        .webhook_passwords(|_| webhook_passwords.clone())
        .build();
    let availability = Availability::default();
    let router = api::router(
        webhooks::register(admin::register(Router::builder())),
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
        reconcile_interval: settings.library_sync.reconcile_interval,
    })
}

impl Server {
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub async fn serve(self, shutdown: impl Future<Output = ()>) -> Result<(), AppError> {
        let availability = self.availability;
        let inbox = tokio::spawn(workers::inbox(self.state.clone()));
        let reconciles = tokio::spawn(workers::reconcile(self.state, self.reconcile_interval));
        availability.accept_traffic();
        tracing::info!(
            server.port = self.listener.local_addr()?.port(),
            "accepting traffic"
        );
        let served = topcoat::serve_until(self.listener, self.router, async {
            shutdown.await;
            tracing::info!("shutdown started, refusing traffic");
            availability.refuse_traffic();
            tokio::time::sleep(DRAIN_PERIOD).await;
        })
        .await;
        inbox.abort();
        reconciles.abort();
        Ok(served?)
    }
}
