use crate::library::Reconciles;
use crate::library::SourceHealth;
use crate::ports::persistence::DbContext;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub reconciles: Reconciles,
    pub source_health: SourceHealth,
}
