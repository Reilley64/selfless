use crate::library::Prober;
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

pub(crate) struct Saved {
    pub(crate) id: Uuid,
    pub(crate) changed: bool,
}

#[ohno::error]
#[display("failed to inspect a media file")]
pub(crate) struct Error;

pub(crate) async fn inspect(
    list_subtitle_files: &ListSubtitleFiles,
    prober: &Prober,
    stored: Option<&MediaFile>,
    path: &str,
    found: Option<FoundMediaFile>,
) -> Result<Inspected, Error> {
    let Some(found) = found else {
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
) -> toasty::Result<Saved> {
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
    Ok(Saved { id, changed })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::AudioStream as ProbedAudioStream;
    use crate::ports::SubtitleStream as ProbedSubtitleStream;
    use crate::ports::VideoStream as ProbedVideoStream;
    use crate::ports::persistence::DbContext;
    use crate::test::MOVIE_FILE;
    use crate::test::MovieLibrary;
    use domain::HdrType;
    use domain::Movie;
    use std::time::Duration;

    fn hdr10_movie() -> Probe {
        Probe {
            container: "matroska,webm".to_owned(),
            duration: Some(Duration::from_millis(6_981_000)),
            video_streams: vec![ProbedVideoStream {
                index: 0,
                codec: "hevc".to_owned(),
                profile: Some("Main 10".to_owned()),
                width: 3840,
                height: 1600,
                frame_rate: Some(24000.0 / 1001.0),
                hdr_type: HdrType::Hdr10,
            }],
            audio_streams: vec![ProbedAudioStream {
                index: 1,
                codec: "truehd".to_owned(),
                language: Some("eng".to_owned()),
                channels: 8,
                title: Some("Dolby TrueHD Atmos 7.1".to_owned()),
                default: true,
            }],
            subtitle_streams: vec![ProbedSubtitleStream {
                index: 2,
                codec: "hdmv_pgs_subtitle".to_owned(),
                language: Some("eng".to_owned()),
                title: Some("Heptapod".to_owned()),
                default: true,
                forced: true,
                hearing_impaired: false,
                text: false,
            }],
        }
    }

    fn sdr_movie() -> Probe {
        Probe {
            container: "mov,mp4,m4a,3gp,3g2,mj2".to_owned(),
            duration: Some(Duration::from_millis(6_980_500)),
            video_streams: vec![ProbedVideoStream {
                index: 0,
                codec: "h264".to_owned(),
                profile: Some("High".to_owned()),
                width: 1920,
                height: 800,
                frame_rate: Some(24000.0 / 1001.0),
                hdr_type: HdrType::Sdr,
            }],
            audio_streams: Vec::new(),
            subtitle_streams: Vec::new(),
        }
    }

    fn subtitle_file(path: &str, language: &str) -> FoundSubtitleFile {
        FoundSubtitleFile {
            path: path.to_owned(),
            language: Some(language.to_owned()),
            hearing_impaired: false,
            forced: false,
        }
    }

    async fn media_file(library: &MovieLibrary) -> MediaFile {
        let mut db_context = library.db_context.clone();
        db_context
            .media_files()
            .filter(MediaFile::fields().path().eq(MOVIE_FILE))
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap()
    }

    async fn streams(library: &MovieLibrary) -> Probe {
        let mut db_context = library.db_context.clone();
        let media_file = media_file(library).await;
        let mut video_streams = db_context
            .video_streams()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        video_streams.sort_by_key(|x| x.index);
        let mut audio_streams = db_context
            .audio_streams()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        audio_streams.sort_by_key(|x| x.index);
        let mut subtitle_streams = db_context
            .subtitle_streams()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        subtitle_streams.sort_by_key(|x| x.index);
        Probe {
            container: media_file.container.unwrap_or_default(),
            duration: media_file
                .duration_ms
                .map(|x| Duration::from_millis(x.try_into().unwrap())),
            video_streams: video_streams
                .into_iter()
                .map(|x| ProbedVideoStream {
                    index: x.index,
                    codec: x.codec,
                    profile: x.profile,
                    width: x.width,
                    height: x.height,
                    frame_rate: x.frame_rate,
                    hdr_type: x.hdr_type,
                })
                .collect(),
            audio_streams: audio_streams
                .into_iter()
                .map(|x| ProbedAudioStream {
                    index: x.index,
                    codec: x.codec,
                    language: x.language,
                    channels: x.channels,
                    title: x.title,
                    default: x.is_default,
                })
                .collect(),
            subtitle_streams: subtitle_streams
                .into_iter()
                .map(|x| ProbedSubtitleStream {
                    index: x.index,
                    codec: x.codec,
                    language: x.language,
                    title: x.title,
                    default: x.is_default,
                    forced: x.forced,
                    hearing_impaired: x.hearing_impaired,
                    text: x.text,
                })
                .collect(),
        }
    }

    async fn problems(library: &MovieLibrary) -> Vec<(SyncProblemKind, Option<Uuid>, String)> {
        let mut db_context = library.db_context.clone();
        db_context
            .sync_problems()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| (x.kind, x.media_file_id, x.path))
            .collect()
    }

    async fn subtitle_files(library: &MovieLibrary) -> Vec<FoundSubtitleFile> {
        let mut db_context = library.db_context.clone();
        let mut subtitle_files = db_context
            .subtitle_files()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| FoundSubtitleFile {
                path: x.path,
                language: x.language,
                hearing_impaired: x.hearing_impaired,
                forced: x.forced,
            })
            .collect::<Vec<_>>();
        subtitle_files.sort();
        subtitle_files
    }

    async fn movie_is_visible(library: &MovieLibrary) -> bool {
        let mut db_context = library.db_context.clone();
        let [movie]: [Movie; 1] = db_context
            .movies()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .try_into()
            .unwrap();
        movie.visible
    }

    async fn inspect_and_save(library: &MovieLibrary) -> bool {
        let mut db_context: DbContext = library.db_context.clone();
        let stored = db_context
            .media_files()
            .filter(MediaFile::fields().path().eq(MOVIE_FILE))
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let found = library.find_media_file().call(MOVIE_FILE).await.unwrap();
        let inspected = inspect(
            &library.list_subtitle_files(),
            &library.prober(),
            stored.as_ref(),
            MOVIE_FILE,
            found,
        )
        .await
        .unwrap();
        save(&mut db_context, MOVIE_FILE, inspected)
            .await
            .unwrap()
            .changed
    }

    #[tokio::test]
    async fn a_new_file_is_probed_once_and_its_streams_are_stored() {
        let library = MovieLibrary::new().await;
        library
            .sources()
            .probes
            .insert(MOVIE_FILE.to_owned(), hdr10_movie());

        library.sync_movie().await.unwrap();

        assert_eq!(streams(&library).await, hdr10_movie());
        assert_eq!(media_file(&library).await.size, Some(1_000));
        assert_eq!(library.sources().probe_calls, 1);
    }

    #[tokio::test]
    async fn a_file_whose_size_or_modification_time_changed_is_probed_again() {
        let library = MovieLibrary::new().await;
        library
            .sources()
            .probes
            .insert(MOVIE_FILE.to_owned(), hdr10_movie());
        library.sync_movie().await.unwrap();
        library.sync_movie().await.unwrap();
        let unchanged_calls = library.sources().probe_calls;

        library.put_file(MOVIE_FILE, 1_000, "2026-10-30T09:00:00Z");
        library
            .sources()
            .probes
            .insert(MOVIE_FILE.to_owned(), sdr_movie());
        library.sync_movie().await.unwrap();

        assert_eq!(unchanged_calls, 1);
        assert_eq!(streams(&library).await, sdr_movie());
        assert_eq!(library.sources().probe_calls, 2);
    }

    #[tokio::test]
    async fn the_stored_subtitle_files_follow_the_listing_and_mark_the_media_file_changed() {
        let library = MovieLibrary::new().await;
        library.sources().subtitle_files = vec![subtitle_file(
            "/media/Movies/Arrival (2016)/Arrival.en.srt",
            "en",
        )];
        inspect_and_save(&library).await;
        let unchanged = inspect_and_save(&library).await;

        library.sources().subtitle_files = vec![subtitle_file(
            "/media/Movies/Arrival (2016)/Arrival.de.srt",
            "de",
        )];
        let changed = inspect_and_save(&library).await;

        assert!(!unchanged);
        assert!(changed);
        assert_eq!(
            subtitle_files(&library).await,
            [subtitle_file(
                "/media/Movies/Arrival (2016)/Arrival.de.srt",
                "de"
            )]
        );
    }

    #[tokio::test]
    async fn a_file_ffprobe_cannot_read_stays_visible_without_streams_until_a_probe_succeeds() {
        let library = MovieLibrary::new().await;
        library.sources().unprobeable.insert(MOVIE_FILE.to_owned());

        library.sync_movie().await.unwrap();
        library.sync_movie().await.unwrap();

        let media_file = media_file(&library).await;
        assert_eq!(media_file.container, None);
        assert!(streams(&library).await.video_streams.is_empty());
        assert!(movie_is_visible(&library).await);
        assert_eq!(
            problems(&library).await,
            [(
                SyncProblemKind::FfprobeFailed,
                Some(media_file.id),
                MOVIE_FILE.to_owned()
            )]
        );
        assert_eq!(library.sources().probe_calls, 1);
        library.sources().unprobeable.clear();
        library
            .sources()
            .probes
            .insert(MOVIE_FILE.to_owned(), hdr10_movie());
        library.put_file(MOVIE_FILE, 1_000, "2026-10-30T09:00:00Z");
        library.sync_movie().await.unwrap();
        assert_eq!(streams(&library).await, hdr10_movie());
        assert_eq!(problems(&library).await, []);
    }

    #[tokio::test]
    async fn a_missing_file_records_file_not_found_until_it_appears() {
        let library = MovieLibrary::new().await;
        library.sources().files_on_disk.clear();

        library.sync_movie().await.unwrap();

        let media_file = media_file(&library).await;
        assert_eq!(
            problems(&library).await,
            [(
                SyncProblemKind::FileNotFound,
                Some(media_file.id),
                MOVIE_FILE.to_owned()
            )]
        );
        library.put_file(MOVIE_FILE, 1_000, "2026-10-29T09:00:00Z");
        library.sync_movie().await.unwrap();
        assert_eq!(problems(&library).await, []);
    }
}
