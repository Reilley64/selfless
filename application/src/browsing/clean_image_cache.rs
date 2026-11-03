mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use std::collections::HashMap;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        list_cached_images,
        delete_cached_image,
    } = dependencies.into();
    let cached = list_cached_images.call().await.map_err(Error::caused_by)?;
    let modified_at = db_context
        .artwork()
        .all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| (x.id, x.modified_at))
        .collect::<HashMap<_, _>>();
    for image in cached {
        if modified_at.get(&image.image_id) != Some(&image.modified_at) {
            delete_cached_image
                .call(&image)
                .await
                .map_err(Error::caused_by)?;
        }
    }
    Ok(Output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::CachedImage;
    use crate::ports::ImageFile;
    use crate::ports::ImageFormat;
    use crate::test::FakeImageCache;
    use crate::test::mock_db_context;
    use domain::ArtworkKind;
    use jiff::Timestamp;
    use uuid::Uuid;

    fn at(timestamp: &str) -> Timestamp {
        timestamp.parse().unwrap()
    }

    #[tokio::test]
    async fn cleaning_deletes_cached_images_whose_source_changed_or_no_longer_exists() {
        let mut db_context = mock_db_context().await;
        let fanart = db_context
            .artwork()
            .create()
            .kind(ArtworkKind::Fanart)
            .path("/mnt/media/movies/Arrival (2016)/fanart.jpg")
            .modified_at(at("2026-11-02T21:30:00Z"))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let cache = FakeImageCache::default();
        let current = [480, 1920].map(|width| CachedImage {
            image_id: fanart.id,
            width,
            modified_at: fanart.modified_at,
        });
        let changed = CachedImage {
            image_id: fanart.id,
            width: 480,
            modified_at: at("2026-10-01T08:00:00Z"),
        };
        let removed = CachedImage {
            image_id: Uuid::new_v4(),
            width: 300,
            modified_at: fanart.modified_at,
        };
        for image in current.iter().chain([&changed, &removed]) {
            cache
                .save()
                .call(
                    image,
                    ImageFile {
                        format: ImageFormat::Jpeg,
                        bytes: "fanart".into(),
                    },
                )
                .await
                .unwrap();
        }

        call(Dependencies {
            db_context: db_context.clone(),
            list_cached_images: cache.list(),
            delete_cached_image: cache.delete(),
        })
        .await
        .unwrap();

        assert_eq!(cache.images(), current);
    }
}
