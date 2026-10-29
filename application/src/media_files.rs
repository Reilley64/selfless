use domain::MediaFile;
use domain::SyncProblem;
use domain::SyncProblemKind;
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
                save_file_not_found(db, media_file.id, path, on_disk).await?;
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
            save_file_not_found(db, media_file.id, path, on_disk).await?;
            Ok((media_file.id, true))
        }
    }
}

async fn save_file_not_found(
    db: &mut dyn toasty::Executor,
    media_file_id: Uuid,
    path: &str,
    on_disk: Option<OnDisk>,
) -> toasty::Result<()> {
    SyncProblem::filter(
        SyncProblem::fields().media_file_id().eq(media_file_id).and(
            SyncProblem::fields()
                .kind()
                .eq(SyncProblemKind::FileNotFound),
        ),
    )
    .delete()
    .exec(db)
    .await?;
    if on_disk.is_none() {
        toasty::create!(SyncProblem {
            movie_id: None,
            media_file_id: Some(media_file_id),
            show_id: None,
            kind: SyncProblemKind::FileNotFound,
            path: path.to_owned(),
        })
        .exec(db)
        .await?;
    }
    Ok(())
}

pub(crate) async fn delete(db: &mut dyn toasty::Executor, ids: Vec<Uuid>) -> toasty::Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    SyncProblem::filter(SyncProblem::fields().media_file_id().in_list(ids.clone()))
        .delete()
        .exec(db)
        .await?;
    MediaFile::filter(MediaFile::fields().id().in_list(ids))
        .delete()
        .exec(db)
        .await
}
