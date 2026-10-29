mod http_server;
mod problem;
mod routes;
mod user_auth;
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
        .route(routes::actuator::health::v1::health)
        .route(routes::actuator::health::v1::liveness)
        .route(routes::actuator::health::v1::readiness)
        .route(routes::actuator::info::v1::info)
        .route(routes::api::sessions::v1::sign_in)
        .route(routes::api::sessions::v1::sign_out)
        .route(routes::api::me::v1::me)
        .route(routes::api::me::v1::change_password)
        .route(routes::api::invites::v1::accept)
        .route(routes::api::telemetry::v1::traces)
        .route(routes::api::telemetry::v1::metrics)
        .route(routes::api::telemetry::v1::logs)
        .route(web::index)
        .route(web::settings)
        .route(web::invite)
        .serve_dir(web::FILES, web_dir)
        .layer(web::ContentSecurityPolicyLayer)
        .layer(problem::BadRequestProblemLayer)
        .layer(user_auth::UserAuthLayer)
        .layer(http_server::HttpServerLayer::new())
        .build()
}
