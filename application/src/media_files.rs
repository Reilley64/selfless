use domain::MediaFile;
use jiff::Timestamp;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(crate) struct OnDisk {
    size: i64,
    modified_at: Timestamp,
}

pub(crate) async fn on_disk(path: &str) -> Option<OnDisk> {
    let metadata = tokio::fs::metadata(path)
        .await
        .ok()
        .filter(|x| x.is_file())?;
    let modified_at = Timestamp::try_from(metadata.modified().ok()?).ok()?;
    Some(OnDisk {
        size: i64::try_from(metadata.len()).ok()?,
        modified_at: Timestamp::from_microsecond(modified_at.as_microsecond()).ok()?,
    })
}

pub(crate) async fn save(
    db: &mut dyn toasty::Executor,
    path: &str,
    on_disk: Option<OnDisk>,
) -> toasty::Result<(Uuid, bool)> {
    let size = on_disk.map(|x| x.size);
    let modified_at = on_disk.map(|x| x.modified_at);
    match MediaFile::filter_by_path(path).first().exec(db).await? {
        Some(mut media_file) => {
            let changed = media_file.size != size || media_file.modified_at != modified_at;
            if changed {
                media_file
                    .update()
                    .size(size)
                    .modified_at(modified_at)
                    .exec(db)
                    .await?;
            }
            Ok((media_file.id, changed))
        }
        None => {
            let media_file = toasty::create!(MediaFile {
                path: path.to_owned(),
                size,
                modified_at,
            })
            .exec(db)
            .await?;
            Ok((media_file.id, true))
        }
    }
}
