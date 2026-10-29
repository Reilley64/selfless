mod actuator;

use infrastructure::AppState;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use topcoat::router::Router;

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

pub fn router(state: AppState, availability: Availability) -> Router {
    Router::builder()
        .app_context(state)
        .app_context(availability)
        .route(actuator::health)
        .route(actuator::liveness)
        .route(actuator::readiness)
        .route(actuator::info)
        .build()
}
