use crate::ports::Clock;
use crate::ports::persistence::DbContext;
use crate::url_signer::UrlSigner;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
    pub url_signer: UrlSigner,
}
