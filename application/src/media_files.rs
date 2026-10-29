use crate::Prober;
use crate::ports::ListSubtitleFiles;
use crate::ports::Probe;
use domain::AudioStream;
use domain::MediaFile;
use domain::SubtitleFile;
use domain::SubtitleStream;
use domain::SyncProblem;
use domain::SyncProblemKind;
use domain::VideoStream;
use jiff::Timestamp;
use uuid::Uuid;

#[derive(Clone, Copy)]
struct OnDisk {
    size: i64,
    modified_at: Timestamp,
}

#[derive(Clone)]
pub(crate) struct Inspector {
    pub(crate) prober: Prober,
    pub(crate) list_subtitle_files: ListSubtitleFiles,
}

pub(crate) struct Inspected {
    on_disk: Option<OnDisk>,
    probe: Option<Option<Probe>>,
    subtitle_files: Vec<crate::ports::SubtitleFile>,
}

impl Inspected {
    pub(crate) fn on_disk(&self) -> bool {
        self.on_disk.is_some()
    }
}

#[ohno::error]
#[display("failed to inspect a media file")]
pub(crate) struct Error;

impl Inspector {
    pub(crate) async fn inspect(
        &self,
        stored: Option<&MediaFile>,
        path: &str,
    ) -> Result<Inspected, Error> {
        let Some(on_disk) = on_disk(path).await else {
            return Ok(Inspected {
                on_disk: None,
                probe: None,
                subtitle_files: Vec::new(),
            });
        };
        let probe = match stored {
            Some(x)
                if x.size == Some(on_disk.size) && x.modified_at == Some(on_disk.modified_at) =>
            {
                None
            }
            _ => Some(match self.prober.probe(path).await {
                Ok(probe) => Some(probe),
                Err(error) => {
                    tracing::warn!(
                        error = &error as &dyn std::error::Error,
                        file.path = path,
                        "ffprobe failed to read a media file"
                    );
                    None
                }
            }),
        };
        let subtitle_files = self
            .list_subtitle_files
            .call(path)
            .await
            .map_err(Error::caused_by)?;
        Ok(Inspected {
            on_disk: Some(on_disk),
            probe,
            subtitle_files,
        })
    }
}

async fn on_disk(path: &str) -> Option<OnDisk> {
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
    inspected: Inspected,
) -> toasty::Result<(Uuid, bool)> {
    let size = inspected.on_disk.map(|x| x.size);
    let modified_at = inspected.on_disk.map(|x| x.modified_at);
    let container = inspected
        .probe
        .as_ref()
        .map(|x| x.as_ref().map(|x| x.container.clone()));
    let duration_ms = inspected.probe.as_ref().map(|x| {
        x.as_ref()
            .and_then(|x| x.duration)
            .and_then(|x| i64::try_from(x.as_millis()).ok())
    });
    let (id, mut changed) = match MediaFile::filter_by_path(path).first().exec(db).await? {
        Some(mut media_file) => {
            let changed = media_file.size != size || media_file.modified_at != modified_at;
            if changed || container.is_some() {
                let container = container.unwrap_or_else(|| media_file.container.clone());
                let duration_ms = duration_ms.unwrap_or(media_file.duration_ms);
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
            let media_file = toasty::create!(MediaFile {
                path: path.to_owned(),
                size,
                modified_at,
                container: container.flatten(),
                duration_ms: duration_ms.flatten(),
            })
            .exec(db)
            .await?;
            (media_file.id, true)
        }
    };
    if changed {
        save_file_not_found(db, id, path, inspected.on_disk).await?;
    }
    if let Some(probe) = inspected.probe {
        save_probe(db, id, path, probe).await?;
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
            movie_id: None,
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
            hdr_type: match stream.hdr_type {
                crate::ports::HdrType::Sdr => domain::HdrType::Sdr,
                crate::ports::HdrType::Hdr10 => domain::HdrType::Hdr10,
                crate::ports::HdrType::Hlg => domain::HdrType::Hlg,
                crate::ports::HdrType::DolbyVision => domain::HdrType::DolbyVision,
            },
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
