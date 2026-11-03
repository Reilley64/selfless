use crate::library::Prober;
use crate::ports::FindMediaFile;
use crate::ports::FoundMediaFile;
use crate::ports::FoundSubtitleFile;
use crate::ports::ListSubtitleFiles;
use crate::ports::Probe;
use crate::ports::persistence::DbScope;
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
    subtitle_files: Vec<FoundSubtitleFile>,
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

pub(crate) async fn inspect(
    find_media_file: &FindMediaFile,
    list_subtitle_files: &ListSubtitleFiles,
    prober: &Prober,
    stored: Option<&MediaFile>,
    path: &str,
) -> Result<Inspected, Error> {
    let Some(found) = find_media_file.call(path).await else {
        return Ok(Inspected {
            found: None,
            probed: Probed::Unchanged,
            subtitle_files: Vec::new(),
        });
    };
    let probed = match stored {
        Some(x) if !x.needs_probe(found.size, found.modified_at) => Probed::Unchanged,
        _ => match prober.probe(path).await {
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
    let subtitle_files = list_subtitle_files
        .call(path)
        .await
        .map_err(Error::caused_by)?;
    Ok(Inspected {
        found: Some(found),
        probed,
        subtitle_files,
    })
}

pub(crate) async fn save(
    scope: &mut impl DbScope,
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
    let (id, mut changed) = match scope
        .media_files()
        .filter(MediaFile::fields().path().eq(path))
        .first()
        .exec(scope.executor_mut())
        .await?
    {
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
                    .exec(scope.executor_mut())
                    .await?;
            }
            (media_file.id, changed)
        }
        None => {
            let (container, duration_ms) = probed.unwrap_or_default();
            let media_file = scope
                .media_files()
                .create()
                .path(path)
                .size(size)
                .modified_at(modified_at)
                .container(container)
                .duration_ms(duration_ms)
                .exec(scope.executor_mut())
                .await?;
            (media_file.id, true)
        }
    };
    if changed {
        save_file_not_found(scope, id, path, inspected.found.is_some()).await?;
    }
    match inspected.probed {
        Probed::Unchanged => {}
        Probed::Failed => save_probe(scope, id, path, None).await?,
        Probed::Read(probe) => save_probe(scope, id, path, Some(probe)).await?,
    }
    changed |= save_subtitle_files(scope, id, inspected.subtitle_files).await?;
    Ok((id, changed))
}

async fn save_probe(
    scope: &mut impl DbScope,
    media_file_id: Uuid,
    path: &str,
    probe: Option<Probe>,
) -> toasty::Result<()> {
    scope
        .video_streams()
        .filter(VideoStream::fields().media_file_id().eq(media_file_id))
        .delete()
        .exec(scope.executor_mut())
        .await?;
    scope
        .audio_streams()
        .filter(AudioStream::fields().media_file_id().eq(media_file_id))
        .delete()
        .exec(scope.executor_mut())
        .await?;
    scope
        .subtitle_streams()
        .filter(SubtitleStream::fields().media_file_id().eq(media_file_id))
        .delete()
        .exec(scope.executor_mut())
        .await?;
    scope
        .sync_problems()
        .filter(
            SyncProblem::fields().media_file_id().eq(media_file_id).and(
                SyncProblem::fields()
                    .kind()
                    .eq(SyncProblemKind::FfprobeFailed),
            ),
        )
        .delete()
        .exec(scope.executor_mut())
        .await?;
    let Some(probe) = probe else {
        scope
            .sync_problems()
            .create()
            .media_file_id(Some(media_file_id))
            .kind(SyncProblemKind::FfprobeFailed)
            .path(path)
            .exec(scope.executor_mut())
            .await?;
        return Ok(());
    };
    for stream in probe.video_streams {
        scope
            .video_streams()
            .create()
            .media_file_id(media_file_id)
            .index(stream.index)
            .codec(stream.codec)
            .profile(stream.profile)
            .width(stream.width)
            .height(stream.height)
            .frame_rate(stream.frame_rate)
            .hdr_type(stream.hdr_type)
            .exec(scope.executor_mut())
            .await?;
    }
    for stream in probe.audio_streams {
        scope
            .audio_streams()
            .create()
            .media_file_id(media_file_id)
            .index(stream.index)
            .codec(stream.codec)
            .language(stream.language)
            .channels(stream.channels)
            .title(stream.title)
            .is_default(stream.default)
            .exec(scope.executor_mut())
            .await?;
    }
    for stream in probe.subtitle_streams {
        scope
            .subtitle_streams()
            .create()
            .media_file_id(media_file_id)
            .index(stream.index)
            .codec(stream.codec)
            .language(stream.language)
            .title(stream.title)
            .is_default(stream.default)
            .forced(stream.forced)
            .hearing_impaired(stream.hearing_impaired)
            .text(stream.text)
            .exec(scope.executor_mut())
            .await?;
    }
    Ok(())
}

async fn save_subtitle_files(
    scope: &mut impl DbScope,
    media_file_id: Uuid,
    mut subtitle_files: Vec<FoundSubtitleFile>,
) -> toasty::Result<bool> {
    subtitle_files.sort();
    let mut stored = scope
        .subtitle_files()
        .filter(SubtitleFile::fields().media_file_id().eq(media_file_id))
        .exec(scope.executor_mut())
        .await?
        .into_iter()
        .map(|x| FoundSubtitleFile {
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
    scope
        .subtitle_files()
        .filter(SubtitleFile::fields().media_file_id().eq(media_file_id))
        .delete()
        .exec(scope.executor_mut())
        .await?;
    for file in subtitle_files {
        scope
            .subtitle_files()
            .create()
            .media_file_id(media_file_id)
            .path(file.path)
            .language(file.language)
            .hearing_impaired(file.hearing_impaired)
            .forced(file.forced)
            .exec(scope.executor_mut())
            .await?;
    }
    Ok(true)
}

async fn save_file_not_found(
    scope: &mut impl DbScope,
    media_file_id: Uuid,
    path: &str,
    found: bool,
) -> toasty::Result<()> {
    scope
        .sync_problems()
        .filter(
            SyncProblem::fields().media_file_id().eq(media_file_id).and(
                SyncProblem::fields()
                    .kind()
                    .eq(SyncProblemKind::FileNotFound),
            ),
        )
        .delete()
        .exec(scope.executor_mut())
        .await?;
    if !found {
        scope
            .sync_problems()
            .create()
            .media_file_id(Some(media_file_id))
            .kind(SyncProblemKind::FileNotFound)
            .path(path)
            .exec(scope.executor_mut())
            .await?;
    }
    Ok(())
}

pub(crate) async fn delete(scope: &mut impl DbScope, ids: Vec<Uuid>) -> toasty::Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    scope
        .sync_problems()
        .filter(SyncProblem::fields().media_file_id().in_list(ids.clone()))
        .delete()
        .exec(scope.executor_mut())
        .await?;
    scope
        .media_files()
        .filter(MediaFile::fields().id().in_list(ids))
        .delete()
        .exec(scope.executor_mut())
        .await
}
