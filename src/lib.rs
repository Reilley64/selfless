use api::Availability;
use application::BuildInfo;
use application::admins::bootstrap_first_admin::MissingFirstAdmin;
use application::browsing::Resizer;
use application::library::Prober;
use application::library::Reconciles;
use application::library::SourceHealth;
use application::ports::Clock;
use application::ports::ProbeMediaFile;
use application::ports::SendEmail;
use application::ports::persistence::DbContext;
use application::url_signer::UrlSigner;
use application::webhooks::WebhookPasswords;
use domain::Password;
use infrastructure::AppState;
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
    reconcile_interval: Duration,
}

pub async fn start(
    settings: AppSettings,
    clock: Clock,
    send_email: SendEmail,
    probe_media_file: ProbeMediaFile,
) -> Result<Server, AppError> {
    let client = servarr::client()?;
    let radarr_api = settings.radarr.api(client.clone())?;
    let webhook_passwords = WebhookPasswords {
        radarr: settings.radarr.webhook_password()?,
        sonarr: settings.sonarr.webhook_password()?,
    };
    let sonarr_api = settings.sonarr.api(client)?;
    let url_signer = UrlSigner::new(settings.signed_urls.key()?.as_bytes());
    let image_cache_dir = settings.images.cache_dir()?;
    let db = persistence::connect(&settings.database).await?;
    persistence::migrate(&db).await?;
    let locks = locking::Locks::default();
    let state = AppState::builder()
        .acquire_lock(|_| locks.acquire_lock())
        .build_info(|_| BuildInfo {
            name: env!("CARGO_PKG_NAME"),
            version: env!("CARGO_PKG_VERSION"),
            git_branch: env!("GIT_BRANCH"),
            git_commit_id: env!("GIT_COMMIT"),
            git_commit_time: env!("GIT_COMMIT_TIME"),
        })
        .check_radarr_kodi_metadata(|_| radarr::check_kodi_metadata(radarr_api.clone()))
        .check_sonarr_kodi_metadata(|_| sonarr::check_kodi_metadata(sonarr_api.clone()))
        .clock(|_| clock.clone())
        .db_context(|_| DbContext::new(db.clone(), settings.database.location()))
        .delete_cached_image(|_| images::delete_cached_image(image_cache_dir.clone()))
        .export_telemetry_try(|_| telemetry::export_telemetry(&settings.otlp))?
        .find_media_file(|_| media_folder::find_media_file())
        .get_radarr_movie(|_| radarr::get_movie(radarr_api.clone()))
        .get_sonarr_episodes(|_| sonarr::get_episodes(sonarr_api.clone()))
        .get_sonarr_show(|_| sonarr::get_show(sonarr_api.clone()))
        .list_cached_images(|_| images::list_cached_images(image_cache_dir.clone()))
        .list_radarr_movies(|_| radarr::list_movies(radarr_api.clone()))
        .list_sonarr_shows(|_| sonarr::list_shows(sonarr_api.clone()))
        .list_subtitle_files(|_| media_folder::list_subtitle_files())
        .prober(|_| Prober::new(probe_media_file.clone()))
        .read_cached_image(|_| images::read_cached_image(image_cache_dir.clone()))
        .read_episode_folder(|_| media_folder::read_episode_folder())
        .read_episode_nfo(|_| media_folder::read_episode_nfo())
        .read_movie_folder(|_| media_folder::read_movie_folder())
        .read_movie_nfo(|_| media_folder::read_movie_nfo())
        .read_radarr_webhook(|_| radarr::read_webhook())
        .read_show_folder(|_| media_folder::read_show_folder())
        .read_show_nfo(|_| media_folder::read_show_nfo())
        .read_sonarr_webhook(|_| sonarr::read_webhook())
        .read_webhook_event_type(|_| servarr::read_webhook_event_type())
        .reconciles(|_| Reconciles::default())
        .resizer(|_| Resizer::new(images::resize_image()))
        .save_cached_image(|_| images::save_cached_image(image_cache_dir.clone()))
        .send_email(|_| send_email.clone())
        .source_health(|_| SourceHealth::default())
        .try_acquire_lock(|_| locks.try_acquire_lock())
        .url_signer(|_| url_signer.clone())
        .webhook_passwords(|_| webhook_passwords.clone())
        .build();
    let first_admin = match (
        settings.admin.username.filter(|x| !x.is_empty()),
        settings.admin.password.map(Password::new),
    ) {
        (Some(name), Some(Ok(password))) => Some((name, password)),
        _ => None,
    };
    application::admins::bootstrap_first_admin::call(&state, first_admin)
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

    pub async fn serve(self, shutdown: impl Future<Output = ()>) -> Result<(), AppError> {
        let availability = self.availability;
        let stop_workers = CancellationToken::new();
        let workers = tokio::spawn(workers::serve(
            self.state,
            self.reconcile_interval,
            stop_workers.clone(),
        ));
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
