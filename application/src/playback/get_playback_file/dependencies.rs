use crate::ports::persistence::DbContext;
use crate::url_signer::UrlSigner;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub url_signer: UrlSigner,
}
