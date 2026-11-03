use crate::browsing::Resizer;
use crate::ports::Clock;
use crate::ports::ReadCachedImage;
use crate::ports::SaveCachedImage;
use crate::ports::persistence::DbContext;
use crate::url_signer::UrlSigner;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
    pub url_signer: UrlSigner,
    pub read_cached_image: ReadCachedImage,
    pub resizer: Resizer,
    pub save_cached_image: SaveCachedImage,
}
