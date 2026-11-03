use crate::ports::Clock;
use crate::ports::EndHlsSession;
use crate::ports::persistence::DbContext;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
    pub end_hls_session: EndHlsSession,
}
