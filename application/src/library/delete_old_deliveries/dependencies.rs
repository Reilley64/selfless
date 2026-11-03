use crate::ports::Clock;
use crate::ports::persistence::DbContext;

#[fundle::deps]
pub struct Dependencies {
    pub clock: Clock,
    pub db_context: DbContext,
}
