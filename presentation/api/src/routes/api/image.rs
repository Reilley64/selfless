use application::browsing::Image;
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
                        x.expires.as_second(),
                        x.signature
                    ),
                })
                .collect(),
        }
    }
}
