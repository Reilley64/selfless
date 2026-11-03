use application::ports::CachedImage;
use application::ports::DeleteCachedImage;
use application::ports::ImageCacheError;
use application::ports::ImageFile;
use application::ports::ImageFormat;
use application::ports::ListCachedImages;
use application::ports::ReadCachedImage;
use application::ports::SaveCachedImage;
use jiff::Timestamp;
use std::io::ErrorKind;
use std::path::PathBuf;
use uuid::Uuid;

pub fn read_cached_image(cache_dir: PathBuf) -> ReadCachedImage {
    ReadCachedImage::new(move |image| {
        let path = cache_dir.join(file_name(&image));
        Box::pin(async move {
            let bytes = match tokio::fs::read(path).await {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(ImageCacheError::caused_by(error)),
            };
            let format = match image::guess_format(&bytes).map_err(ImageCacheError::caused_by)? {
                image::ImageFormat::Jpeg => ImageFormat::Jpeg,
                image::ImageFormat::Png => ImageFormat::Png,
                _ => {
                    return Err(ImageCacheError::caused_by(
                        "the cached image is not a JPEG or PNG",
                    ));
                }
            };
            Ok(Some(ImageFile {
                format,
                bytes: bytes.into(),
            }))
        })
    })
}

pub fn save_cached_image(cache_dir: PathBuf) -> SaveCachedImage {
    SaveCachedImage::new(move |image, file| {
        let cache_dir = cache_dir.clone();
        Box::pin(async move {
            let path = cache_dir.join(file_name(&image));
            let partial = cache_dir.join(format!("{}.partial", Uuid::new_v4()));
            tokio::fs::create_dir_all(&cache_dir)
                .await
                .map_err(ImageCacheError::caused_by)?;
            tokio::fs::write(&partial, &file.bytes)
                .await
                .map_err(ImageCacheError::caused_by)?;
            tokio::fs::rename(&partial, path)
                .await
                .map_err(ImageCacheError::caused_by)
        })
    })
}

pub fn list_cached_images(cache_dir: PathBuf) -> ListCachedImages {
    ListCachedImages::new(move || {
        let cache_dir = cache_dir.clone();
        Box::pin(async move {
            let mut entries = match tokio::fs::read_dir(cache_dir).await {
                Ok(entries) => entries,
                Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
                Err(error) => return Err(ImageCacheError::caused_by(error)),
            };
            let mut images = Vec::new();
            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(ImageCacheError::caused_by)?
            {
                if let Some(image) = entry.file_name().to_str().and_then(cached_image) {
                    images.push(image);
                }
            }
            Ok(images)
        })
    })
}

pub fn delete_cached_image(cache_dir: PathBuf) -> DeleteCachedImage {
    DeleteCachedImage::new(move |image| {
        let path = cache_dir.join(file_name(&image));
        Box::pin(async move {
            match tokio::fs::remove_file(path).await {
                Err(error) if error.kind() != ErrorKind::NotFound => {
                    Err(ImageCacheError::caused_by(error))
                }
                _ => Ok(()),
            }
        })
    })
}

fn file_name(image: &CachedImage) -> String {
    format!(
        "{}_{}_{}",
        image.image_id,
        image.width,
        image.modified_at.as_nanosecond()
    )
}

fn cached_image(file_name: &str) -> Option<CachedImage> {
    let mut parts = file_name.split('_');
    let image = CachedImage {
        image_id: parts.next()?.parse().ok()?,
        width: parts.next()?.parse().ok()?,
        modified_at: Timestamp::from_nanosecond(parts.next()?.parse().ok()?).ok()?,
    };
    parts.next().is_none().then_some(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_saved_image_reads_back_and_is_listed_until_it_is_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("images");
        let image = CachedImage {
            image_id: Uuid::new_v4(),
            width: 300,
            modified_at: "2026-11-02T21:30:00.123456Z".parse().unwrap(),
        };
        let logo = ImageFile {
            format: ImageFormat::Png,
            bytes: std::fs::read(format!(
                "{}/../../test/fixtures/images/clearlogo.png",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap()
            .into(),
        };

        let before = read_cached_image(cache_dir.clone())
            .call(&image)
            .await
            .unwrap();
        save_cached_image(cache_dir.clone())
            .call(&image, logo.clone())
            .await
            .unwrap();
        let saved = read_cached_image(cache_dir.clone())
            .call(&image)
            .await
            .unwrap();
        let listed = list_cached_images(cache_dir.clone()).call().await.unwrap();
        delete_cached_image(cache_dir.clone())
            .call(&image)
            .await
            .unwrap();
        let after = read_cached_image(cache_dir.clone())
            .call(&image)
            .await
            .unwrap();

        assert_eq!(before, None);
        assert_eq!(saved, Some(logo));
        assert_eq!(listed, [image]);
        assert_eq!(after, None);
        assert_eq!(list_cached_images(cache_dir).call().await.unwrap(), []);
    }

    #[tokio::test]
    async fn listing_a_cache_folder_that_does_not_exist_yet_finds_nothing() {
        let dir = tempfile::tempdir().unwrap();

        let listed = list_cached_images(dir.path().join("images"))
            .call()
            .await
            .unwrap();

        assert_eq!(listed, []);
    }
}
