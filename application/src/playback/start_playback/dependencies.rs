use crate::ports::Clock;
use crate::ports::EndHlsSession;
use crate::ports::ReadKeyframes;
use crate::ports::StartHlsSession;
use crate::ports::persistence::DbContext;
use crate::url_signer::UrlSigner;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
    pub url_signer: UrlSigner,
    pub read_keyframes: ReadKeyframes,
    pub start_hls_session: StartHlsSession,
    pub end_hls_session: EndHlsSession,
}
