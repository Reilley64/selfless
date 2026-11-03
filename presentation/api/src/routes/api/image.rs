use application::browsing::Image;
use application::browsing::TileImages;
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct ImageBody {
    urls: Vec<ImageUrlBody>,
}

#[derive(Serialize)]
struct ImageUrlBody {
    width: i64,
    url: String,
}

impl From<Image> for ImageBody {
    fn from(image: Image) -> Self {
        Self {
            urls: image
                .urls
                .into_iter()
                .map(|x| ImageUrlBody {
                    width: x.width,
                    url: format!(
                        "/api/images/{}/{}/v1?expires={}&signature={}",
                        image.id,
                        x.width,
                        x.signed_url.expires.as_second(),
                        x.signed_url.signature
                    ),
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct TitleImagesBody {
    fanart: Option<ImageBody>,
    logo: Option<ImageBody>,
}

impl From<TileImages> for TitleImagesBody {
    fn from(images: TileImages) -> Self {
        Self {
            fanart: images.fanart.map(ImageBody::from),
            logo: images.logo.map(ImageBody::from),
        }
    }
}
