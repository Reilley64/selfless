use crate::ports::AcquireLock;
use crate::ports::persistence::DbContext;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub acquire_lock: AcquireLock,
}
