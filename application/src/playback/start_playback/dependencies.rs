use crate::ports::Clock;
use crate::ports::EndHlsOutput;
use crate::ports::ReadKeyframes;
use crate::ports::StartHlsOutput;
use crate::ports::persistence::DbContext;
use crate::url_signer::UrlSigner;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
    pub url_signer: UrlSigner,
    pub read_keyframes: ReadKeyframes,
    pub start_hls_output: StartHlsOutput,
    pub end_hls_output: EndHlsOutput,
}
