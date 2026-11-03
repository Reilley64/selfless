use crate::ports::EndHlsSession;
use crate::ports::persistence::DbContext;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub end_hls_session: EndHlsSession,
}
