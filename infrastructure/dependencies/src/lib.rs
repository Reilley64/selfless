use application::DbContext;

pub use persistence;
pub use telemetry;

#[fundle::bundle]
#[derive(Clone)]
pub struct AppState {
    pub db_context: DbContext,
}
