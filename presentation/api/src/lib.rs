mod actuator;
mod http_server;
mod problem;
mod sessions;
mod telemetry;
mod user_auth;
mod users;
mod web;

use infrastructure::AppState;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use topcoat::router::Router;
use topcoat::router::RouterBuilder;
use topcoat::router::RouterBuilderDirectoryExt;

#[derive(Clone, Default)]
pub struct Availability {
    accepting_traffic: Arc<AtomicBool>,
}

impl Availability {
    pub fn accept_traffic(&self) {
        self.accepting_traffic.store(true, Ordering::SeqCst);
    }

    pub fn refuse_traffic(&self) {
        self.accepting_traffic.store(false, Ordering::SeqCst);
    }

    fn is_accepting_traffic(&self) -> bool {
        self.accepting_traffic.load(Ordering::SeqCst)
    }
}

pub fn router(
    router: RouterBuilder,
    state: AppState,
    availability: Availability,
    web_dir: PathBuf,
) -> Router {
    router
        .app_context(state)
        .app_context(availability)
        .route(actuator::health)
        .route(actuator::liveness)
        .route(actuator::readiness)
        .route(actuator::info)
        .route(sessions::sign_in)
        .route(sessions::sign_out)
        .route(users::me)
        .route(users::change_password)
        .route(telemetry::traces)
        .route(telemetry::metrics)
        .route(telemetry::logs)
        .route(web::index)
        .route(web::settings)
        .serve_dir(web::FILES, web_dir)
        .layer(web::ContentSecurityPolicyLayer)
        .layer(user_auth::UserAuthLayer)
        .layer(http_server::HttpServerLayer::new())
        .build()
}
