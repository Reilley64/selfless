use crate::MediaFolder;
use crate::ports::FoundMediaFile;
use crate::ports::Probe;
use domain::AudioStream;
use domain::MediaFile;
use domain::SubtitleFile;
use domain::SubtitleStream;
use domain::SyncProblem;
use domain::SyncProblemKind;
use domain::VideoStream;
use uuid::Uuid;

pub(crate) struct Inspected {
    found: Option<FoundMediaFile>,
    probed: Probed,
    subtitle_files: Vec<crate::ports::SubtitleFile>,
}

enum Probed {
    Unchanged,
    Failed,
    Read(Probe),
}

impl Inspected {
    pub(crate) fn on_disk(&self) -> bool {
        self.found.is_some()
    }
}

#[ohno::error]
#[display("failed to inspect a media file")]
pub(crate) struct Error;

impl MediaFolder {
    pub(crate) async fn inspect(
        &self,
        stored: Option<&MediaFile>,
        path: &str,
    ) -> Result<Inspected, Error> {
        let Some(found) = self.find_media_file.call(path).await else {
            return Ok(Inspected {
                found: None,
                probed: Probed::Unchanged,
                subtitle_files: Vec::new(),
            });
        };
        let probed = match stored {
            Some(x) if x.size == Some(found.size) && x.modified_at == Some(found.modified_at) => {
                Probed::Unchanged
            }
            _ => match self.prober.probe(path).await {
                Ok(probe) => Probed::Read(probe),
                Err(error) => {
                    tracing::warn!(
                        error = &error as &dyn std::error::Error,
                        file.path = path,
                        "ffprobe failed to read a media file"
                    );
                    Probed::Failed
                }
            },
        };
        let subtitle_files = self
            .list_subtitle_files
            .call(path)
            .await
            .map_err(Error::caused_by)?;
        Ok(Inspected {
            found: Some(found),
            probed,
            subtitle_files,
        })
    }
}

pub(crate) async fn save(
    db: &mut dyn toasty::Executor,
    path: &str,
    inspected: Inspected,
) -> toasty::Result<(Uuid, bool)> {
    let size = inspected.found.map(|x| x.size);
    let modified_at = inspected.found.map(|x| x.modified_at);
    let probed = match &inspected.probed {
        Probed::Unchanged => None,
        Probed::Failed => Some((None, None)),
        Probed::Read(probe) => Some((
            Some(probe.container.clone()),
            probe
                .duration
                .and_then(|x| i64::try_from(x.as_millis()).ok()),
        )),
    };
    let (id, mut changed) = match MediaFile::filter_by_path(path).first().exec(db).await? {
        Some(mut media_file) => {
            let changed = media_file.size != size || media_file.modified_at != modified_at;
            if changed || probed.is_some() {
                let (container, duration_ms) = probed
                    .unwrap_or_else(|| (media_file.container.clone(), media_file.duration_ms));
                media_file
                    .update()
                    .size(size)
                    .modified_at(modified_at)
                    .container(container)
                    .duration_ms(duration_ms)
                    .exec(db)
                    .await?;
            }
            (media_file.id, changed)
        }
        None => {
            let (container, duration_ms) = probed.unwrap_or_default();
            let media_file = toasty::create!(MediaFile {
                path: path.to_owned(),
                size,
                modified_at,
                container,
                duration_ms,
            })
            .exec(db)
            .await?;
            (media_file.id, true)
        }
    };
    if changed {
        save_file_not_found(db, id, path, inspected.found.is_some()).await?;
    }
    match inspected.probed {
        Probed::Unchanged => {}
        Probed::Failed => save_probe(db, id, path, None).await?,
        Probed::Read(probe) => save_probe(db, id, path, Some(probe)).await?,
    }
    changed |= save_subtitle_files(db, id, inspected.subtitle_files).await?;
    Ok((id, changed))
}

async fn save_probe(
    db: &mut dyn toasty::Executor,
    media_file_id: Uuid,
    path: &str,
    probe: Option<Probe>,
) -> toasty::Result<()> {
    VideoStream::filter_by_media_file_id(media_file_id)
        .delete()
        .exec(db)
        .await?;
    AudioStream::filter_by_media_file_id(media_file_id)
        .delete()
        .exec(db)
        .await?;
    SubtitleStream::filter_by_media_file_id(media_file_id)
        .delete()
        .exec(db)
        .await?;
    SyncProblem::filter_by_media_file_id(media_file_id)
        .filter(
            SyncProblem::fields()
                .kind()
                .eq(SyncProblemKind::FfprobeFailed),
        )
        .delete()
        .exec(db)
        .await?;
    let Some(probe) = probe else {
        toasty::create!(SyncProblem {
            media_file_id: Some(media_file_id),
            kind: SyncProblemKind::FfprobeFailed,
            path: path.to_owned(),
        })
        .exec(db)
        .await?;
        return Ok(());
    };
    for stream in probe.video_streams {
        toasty::create!(VideoStream {
            media_file_id,
            index: stream.index,
            codec: stream.codec,
            profile: stream.profile,
            width: stream.width,
            height: stream.height,
            frame_rate: stream.frame_rate,
            hdr_type: stream.hdr_type,
        })
        .exec(db)
        .await?;
    }
    for stream in probe.audio_streams {
        toasty::create!(AudioStream {
            media_file_id,
            index: stream.index,
            codec: stream.codec,
            language: stream.language,
            channels: stream.channels,
            title: stream.title,
            is_default: stream.default,
        })
        .exec(db)
        .await?;
    }
    for stream in probe.subtitle_streams {
        toasty::create!(SubtitleStream {
            media_file_id,
            index: stream.index,
            codec: stream.codec,
            language: stream.language,
            title: stream.title,
            is_default: stream.default,
            forced: stream.forced,
            hearing_impaired: stream.hearing_impaired,
            text: stream.text,
        })
        .exec(db)
        .await?;
    }
    Ok(())
}

async fn save_subtitle_files(
    db: &mut dyn toasty::Executor,
    media_file_id: Uuid,
    mut subtitle_files: Vec<crate::ports::SubtitleFile>,
) -> toasty::Result<bool> {
    subtitle_files.sort();
    let mut stored = SubtitleFile::filter_by_media_file_id(media_file_id)
        .exec(db)
        .await?
        .into_iter()
        .map(|x| crate::ports::SubtitleFile {
            path: x.path,
            language: x.language,
            hearing_impaired: x.hearing_impaired,
            forced: x.forced,
        })
        .collect::<Vec<_>>();
    stored.sort();
    if stored == subtitle_files {
        return Ok(false);
    }
    SubtitleFile::filter_by_media_file_id(media_file_id)
        .delete()
        .exec(db)
        .await?;
    for file in subtitle_files {
        toasty::create!(SubtitleFile {
            media_file_id,
            path: file.path,
            language: file.language,
            hearing_impaired: file.hearing_impaired,
            forced: file.forced,
        })
        .exec(db)
        .await?;
    }
    Ok(true)
}

async fn save_file_not_found(
    db: &mut dyn toasty::Executor,
    media_file_id: Uuid,
    path: &str,
    found: bool,
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
    if !found {
        toasty::create!(SyncProblem {
            media_file_id: Some(media_file_id),
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
