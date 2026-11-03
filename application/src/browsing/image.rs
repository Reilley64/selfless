use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use crate::url_signer::SignedUrl;
use crate::url_signer::UrlSigner;
use domain::Artwork;
use domain::ArtworkKind;
use jiff::Timestamp;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Image {
    pub id: Uuid,
    pub urls: Vec<ImageUrl>,
}

#[derive(Clone, Debug)]
pub struct ImageUrl {
    pub width: i64,
    pub signed_url: SignedUrl,
}

#[derive(Clone, Debug, Default)]
pub struct TileImages {
    pub fanart: Option<Image>,
    pub logo: Option<Image>,
    pub thumb: Option<Image>,
}

impl TileImages {
    pub(crate) fn add(&mut self, artwork: &Artwork, url_signer: &UrlSigner, now: Timestamp) {
        let slot = match artwork.kind {
            ArtworkKind::Fanart => &mut self.fanart,
            ArtworkKind::Clearlogo => &mut self.logo,
            ArtworkKind::Thumb => &mut self.thumb,
            ArtworkKind::Poster => return,
        };
        *slot = Some(Image::new(artwork, url_signer, now));
    }
}

impl Image {
    pub(crate) fn new(artwork: &Artwork, url_signer: &UrlSigner, now: Timestamp) -> Self {
        Self {
            id: artwork.id,
            urls: artwork
                .kind
                .widths()
                .iter()
                .map(|&width| ImageUrl {
                    width,
                    signed_url: url_signer
                        .sign(&resource(artwork.id, width, artwork.modified_at), now),
                })
                .collect(),
        }
    }
}

pub(crate) async fn title_images(
    db_context: &mut DbContext,
    owner: impl Fn(&Artwork) -> Option<Uuid>,
    owned: toasty::stmt::Expr<bool>,
    url_signer: &UrlSigner,
    now: Timestamp,
) -> toasty::Result<HashMap<Uuid, TileImages>> {
    let mut images = HashMap::<Uuid, TileImages>::new();
    for artwork in db_context
        .artwork()
        .filter(
            owned.and(
                Artwork::fields()
                    .kind()
                    .is_fanart()
                    .or(Artwork::fields().kind().is_clearlogo()),
            ),
        )
        .exec(db_context.executor_mut())
        .await?
    {
        if let Some(id) = owner(&artwork) {
            images.entry(id).or_default().add(&artwork, url_signer, now);
        }
    }
    Ok(images)
}

pub(crate) fn resource(id: Uuid, width: i64, modified_at: Timestamp) -> String {
    format!("images/{id}/{width}/{}", modified_at.as_nanosecond())
}
