use std::error::Error;
use tokio_postgres::error::DbError;
use tokio_postgres::error::SqlState;

pub(crate) fn violated(error: &toasty::Error) -> Option<&str> {
    std::iter::successors(Some(error as &(dyn Error + 'static)), |&x| x.source())
        .find_map(|x| x.downcast_ref::<DbError>())
        .filter(|x| *x.code() == SqlState::UNIQUE_VIOLATION)
        .and_then(DbError::constraint)
}
