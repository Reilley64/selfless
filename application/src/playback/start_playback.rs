mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::AudioTrack;
pub use output::Episode;
pub use output::Movie;
pub use output::NextEpisode;
pub use output::Output;
pub use output::Playback;
pub use output::SubtitleTrack;

use crate::attributes::SELFLESS_AUTH_CLIENT_KIND;
use crate::attributes::SELFLESS_PLAYBACK_METHOD;
use crate::attributes::SELFLESS_PLAYBACK_REASON_KINDS;
use crate::browsing::Image;
use crate::metrics::SELFLESS_PLAYBACK_DECISIONS;
use crate::playback::resource;
use crate::ports;
use crate::ports::HlsAudioTrack;
use crate::ports::HlsOutput;
use crate::ports::ReadKeyframes;
use crate::ports::persistence::DbScope;
use crate::telemetry::attribute_values::client_kind;
use crate::telemetry::attribute_values::play_method;
use crate::telemetry::attribute_values::reason_kinds;
use crate::url_signer::UrlSigner;
use domain::Artwork;
use domain::AudioStream;
use domain::CapabilityProfile;
use domain::ClientKind;
use domain::KeyframeIndex;
use domain::MediaFile;
use domain::PlayMethod;
use domain::PlayReason;
use domain::PlaybackSettings;
use domain::ShowEpisode;
use domain::usable_keyframe_index;
use jiff::Timestamp;
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::Counter;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

static DECISIONS: LazyLock<Counter<u64>> = LazyLock::new(|| {
    global::meter("selfless")
        .u64_counter(SELFLESS_PLAYBACK_DECISIONS)
        .with_unit("{decision}")
        .build()
});

#[allow(clippy::too_many_arguments)]
pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    session_id: Uuid,
    session_client_kind: ClientKind,
    media_file_id: Uuid,
    start_position: f64,
    audio_track_id: Option<Uuid>,
    subtitle_track_id: Option<Option<Uuid>>,
    profile: &CapabilityProfile,
    force_hls: bool,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        url_signer,
        read_keyframes,
        start_hls_output,
        end_hls_output,
    } = dependencies.into();
    let Some(media_file) = db_context
        .media_files()
        .find(media_file_id)
        .include(MediaFile::fields().video_streams())
        .include(MediaFile::fields().audio_streams())
        .include(MediaFile::fields().subtitle_streams())
        .include(MediaFile::fields().subtitle_files())
        .include(MediaFile::fields().keyframe_index())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(Output::NotFound);
    };
    let movie = db_context
        .movies()
        .filter(
            domain::Movie::fields()
                .media_file_id()
                .eq(media_file_id)
                .and(domain::Movie::in_library_filter()),
        )
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let episodes = db_context
        .episodes()
        .filter(
            domain::Episode::fields()
                .media_file_id()
                .eq(media_file_id)
                .and(domain::Episode::in_library_filter()),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    if movie.is_none() && episodes.is_empty() {
        return Ok(Output::NotFound);
    }
    let Some(container) = media_file.container_name().map(str::to_owned) else {
        return Ok(Output::UnreadableMediaFile);
    };
    let duration = media_file
        .duration_ms
        .and_then(|x| u64::try_from(x).ok())
        .map(Duration::from_millis);
    let video = media_file
        .video_streams
        .get()
        .iter()
        .min_by_key(|x| x.index);
    PlaybackSettings::upsert_by_user_id(user_id)
        .or_ignore()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let settings = db_context
        .playback_settings()
        .filter(PlaybackSettings::fields().user_id().eq(user_id))
        .one()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let PlaybackSettings {
        audio_language,
        subtitle_language,
        autoplay_next_episode,
        ask_if_still_watching,
        ..
    } = settings;
    let audio_streams = media_file.audio_streams.get();
    let default_audio = AudioStream::selected_by_default(audio_streams, audio_language.as_deref());
    let audio = audio_track_id
        .and_then(|id| audio_streams.iter().find(|x| x.id == id))
        .or(default_audio);
    let subtitle_tracks = domain::SubtitleTrack::all(
        media_file.subtitle_streams.get(),
        media_file.subtitle_files.get(),
    );
    let default_subtitle = |audio| {
        domain::SubtitleTrack::selected_by_default(
            &subtitle_tracks,
            profile,
            subtitle_language.as_deref(),
            audio,
        )
    };
    let subtitle = match subtitle_track_id {
        Some(id) => id.and_then(|id| subtitle_tracks.iter().copied().find(|x| x.id() == id)),
        None => default_subtitle(audio),
    };
    let default_audio_track_id = default_audio.map(|x| x.id);
    let default_subtitle_track_id = default_subtitle(default_audio).map(|x| x.id());
    let audio_track_id = audio.map(|x| x.id);
    let subtitle_track_id = subtitle.map(|x| x.id());
    let mut reasons = profile.direct_play_reasons(&container, video, audio, subtitle);
    for reason in profile.tv_subtitle_reasons(
        session_client_kind,
        media_file.subtitle_streams.get(),
        media_file.subtitle_files.get(),
    ) {
        if !reasons.contains(&reason) {
            reasons.push(reason);
        }
    }
    if force_hls && reasons.is_empty() {
        reasons.push(PlayReason::ForcedHls);
    }
    let hls = match (video, duration) {
        (Some(video), Some(duration)) if !reasons.is_empty() => {
            let keyframes = if reasons.iter().any(PlayReason::transcodes_video) {
                Vec::new()
            } else {
                let keyframes = keyframes(&mut db_context, &read_keyframes, &media_file).await?;
                if usable_keyframe_index(&keyframes) {
                    keyframes
                } else {
                    reasons.push(PlayReason::NoKeyframeIndex);
                    Vec::new()
                }
            };
            let video_transcode = reasons
                .iter()
                .any(PlayReason::transcodes_video)
                .then(|| video.transcode());
            Some((video, duration, keyframes, video_transcode))
        }
        _ => None,
    };
    let mut audio_tracks = Vec::new();
    if hls.is_some() {
        let mut streams = media_file.audio_streams.get().iter().collect::<Vec<_>>();
        streams.sort_by_key(|x| x.index);
        for stream in streams {
            let transcode = profile.audio_reason(stream).map(|reason| {
                if !reasons.contains(&reason) {
                    reasons.push(reason);
                }
                profile.audio_transcode(stream)
            });
            audio_tracks.push(HlsAudioTrack {
                stream: ports::AudioStream {
                    index: stream.index,
                    codec: stream.codec.clone(),
                    language: stream.language.clone(),
                    channels: stream.channels,
                    title: stream.title.clone(),
                    default: stream.is_default,
                },
                transcode,
                selected: audio_track_id == Some(stream.id),
            });
        }
    }
    if !reasons.is_empty() {
        Span::current().set_attribute(SELFLESS_PLAYBACK_REASON_KINDS, reason_kinds(&reasons));
        if hls.is_none() {
            return Ok(Output::NeedsTranscode);
        }
    }
    let method = match &hls {
        None => PlayMethod::DirectPlay,
        Some((_, _, _, None)) if audio_tracks.iter().all(|x| x.transcode.is_none()) => {
            PlayMethod::Remux
        }
        Some(_) => PlayMethod::Transcode,
    };
    let now = clock.call();
    let next_episode = next_episode(&mut db_context, &episodes, &url_signer, now)
        .await
        .map_err(Error::caused_by)?;
    let episodes = player_episodes(&mut db_context, episodes)
        .await
        .map_err(Error::caused_by)?;
    let mut subtitle_streams = media_file.subtitle_streams.into_inner();
    let mut subtitle_files = media_file.subtitle_files.into_inner();
    subtitle_streams.sort_by_key(|x| x.index);
    subtitle_files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let mut ended = Vec::new();
    for mut open in transaction
        .playbacks()
        .filter(
            domain::Playback::fields()
                .session_id()
                .eq(session_id)
                .and(domain::Playback::open_filter()),
        )
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?
    {
        open.update()
            .ended_at(Some(now))
            .exec(transaction.executor_mut())
            .await
            .map_err(Error::caused_by)?;
        ended.push(open.id);
    }
    let playback = transaction
        .playbacks()
        .create()
        .user_id(user_id)
        .session_id(session_id)
        .media_file_id(media_file_id)
        .method(method)
        .position(start_position)
        .paused(false)
        .audio_track_id(default_audio_track_id)
        .subtitle_track_id(default_subtitle_track_id)
        .started_at(now)
        .last_reported_at(now)
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    for id in ended {
        end_hls_output.call(id).await;
    }
    if let Some((video, duration, keyframes, video_transcode)) = hls {
        let selected_subtitle = subtitle_streams
            .iter()
            .filter(|x| x.text)
            .map(|x| x.id)
            .chain(subtitle_files.iter().map(|x| x.id))
            .position(|x| Some(x) == subtitle_track_id);
        start_hls_output
            .call(HlsOutput {
                playback_id: playback.id,
                path: media_file.path.clone(),
                duration,
                keyframes,
                video_transcode,
                video: ports::VideoStream {
                    index: video.index,
                    codec: video.codec.clone(),
                    profile: video.profile.clone(),
                    width: video.width,
                    height: video.height,
                    frame_rate: video.frame_rate,
                    hdr_type: video.hdr_type,
                    interlaced: video.interlaced,
                    bit_rate: video.bit_rate,
                },
                audio: audio_tracks,
                subtitle_streams: subtitle_streams
                    .iter()
                    .filter(|x| x.text)
                    .map(|x| ports::SubtitleStream {
                        index: x.index,
                        codec: x.codec.clone(),
                        language: x.language.clone(),
                        title: x.title.clone(),
                        default: x.is_default,
                        forced: x.forced,
                        hearing_impaired: x.hearing_impaired,
                        text: x.text,
                    })
                    .collect(),
                subtitle_files: subtitle_files
                    .iter()
                    .map(|x| ports::FoundSubtitleFile {
                        path: x.path.clone(),
                        language: x.language.clone(),
                        hearing_impaired: x.hearing_impaired,
                        forced: x.forced,
                    })
                    .collect(),
                selected_subtitle,
                start_position: Duration::try_from_secs_f64(start_position).unwrap_or_default(),
            })
            .await
            .map_err(Error::caused_by)?;
    }
    Span::current().set_attribute(SELFLESS_PLAYBACK_METHOD, play_method(playback.method));
    let mut decision = vec![
        KeyValue::new(SELFLESS_PLAYBACK_METHOD, play_method(playback.method)),
        KeyValue::new(SELFLESS_AUTH_CLIENT_KIND, client_kind(session_client_kind)),
    ];
    if !reasons.is_empty() {
        decision.push(KeyValue::new(
            SELFLESS_PLAYBACK_REASON_KINDS,
            reason_kinds(&reasons),
        ));
    }
    DECISIONS.add(1, &decision);
    let mut audio_streams = media_file.audio_streams.into_inner();
    audio_streams.sort_by_key(|x| x.index);
    let hls = playback.method != PlayMethod::DirectPlay;
    Ok(Output::Playback(Box::new(Playback {
        id: playback.id,
        method: playback.method,
        reasons,
        signature: url_signer.signature(&resource(playback.id)),
        duration,
        movie: movie.map(|x| Movie {
            id: x.id,
            title: x.title,
        }),
        episodes,
        audio_track_id,
        subtitle_track_id,
        audio_tracks: audio_streams
            .into_iter()
            .map(|x| AudioTrack {
                id: x.id,
                language: x.language,
                label: x.title,
                codec: x.codec,
                channels: x.channels,
                default: x.is_default,
                in_stream: true,
            })
            .collect(),
        subtitle_tracks: subtitle_streams
            .into_iter()
            .map(|x| SubtitleTrack {
                in_stream: if hls {
                    x.text
                } else {
                    profile.subtitle_codecs.contains(&x.codec)
                },
                id: x.id,
                language: x.language,
                label: x.title,
                codec: x.codec,
                default: x.is_default,
                forced: x.forced,
                hearing_impaired: x.hearing_impaired,
                external: false,
                text: x.text,
            })
            .chain(subtitle_files.into_iter().map(|x| SubtitleTrack {
                codec: x.codec(),
                id: x.id,
                language: x.language,
                label: None,
                default: false,
                forced: x.forced,
                hearing_impaired: x.hearing_impaired,
                external: true,
                text: true,
                in_stream: hls,
            }))
            .collect(),
        next_episode,
        autoplay_next_episode,
        ask_if_still_watching,
    })))
}

async fn keyframes(
    db_context: &mut impl DbScope,
    read_keyframes: &ReadKeyframes,
    media_file: &MediaFile,
) -> Result<Vec<Duration>, Error> {
    if let Some(index) = media_file
        .keyframe_index
        .get()
        .as_ref()
        .filter(|x| x.is_current_for(media_file))
    {
        return Ok(index.keyframes());
    }
    let keyframes = match read_keyframes.call(&media_file.path).await {
        Ok(keyframes) => keyframes,
        Err(error) => {
            tracing::warn!(
                error = &error as &dyn std::error::Error,
                file.path = media_file.path,
                "failed to read the keyframes of a media file"
            );
            return Ok(Vec::new());
        }
    };
    KeyframeIndex::upsert_by_media_file_id(media_file.id)
        .size(media_file.size)
        .modified_at(media_file.modified_at)
        .keyframes_ns(KeyframeIndex::nanoseconds(&keyframes))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(keyframes)
}

async fn player_episodes(
    db_context: &mut impl DbScope,
    episodes: Vec<domain::Episode>,
) -> toasty::Result<Vec<Episode>> {
    if episodes.is_empty() {
        return Ok(Vec::new());
    }
    let seasons: HashMap<Uuid, domain::Season> = db_context
        .seasons()
        .filter(
            domain::Season::fields()
                .id()
                .in_list(episodes.iter().map(|x| x.season_id).collect::<Vec<_>>()),
        )
        .exec(db_context.executor_mut())
        .await?
        .into_iter()
        .map(|x| (x.id, x))
        .collect();
    let shows: HashMap<Uuid, String> = db_context
        .shows()
        .filter(
            domain::Show::fields()
                .id()
                .in_list(seasons.values().map(|x| x.show_id).collect::<Vec<_>>()),
        )
        .exec(db_context.executor_mut())
        .await?
        .into_iter()
        .map(|x| (x.id, x.title))
        .collect();
    let mut episodes = episodes
        .into_iter()
        .filter_map(|x| {
            let season = seasons.get(&x.season_id)?;
            Some(Episode {
                id: x.id,
                show_title: shows.get(&season.show_id)?.clone(),
                season_number: season.number,
                episode_number: x.number,
                title: x.title,
            })
        })
        .collect::<Vec<_>>();
    episodes.sort_by_key(|x| (x.season_number, x.episode_number));
    Ok(episodes)
}

async fn next_episode(
    db_context: &mut impl DbScope,
    playing: &[domain::Episode],
    url_signer: &UrlSigner,
    now: Timestamp,
) -> toasty::Result<Option<NextEpisode>> {
    let Some(first) = playing.first() else {
        return Ok(None);
    };
    let show_id = db_context
        .seasons()
        .filter(domain::Season::fields().id().eq(first.season_id))
        .get(db_context.executor_mut())
        .await?
        .show_id;
    let seasons: HashMap<Uuid, domain::Season> = db_context
        .seasons()
        .filter(domain::Season::fields().show_id().eq(show_id))
        .exec(db_context.executor_mut())
        .await?
        .into_iter()
        .map(|x| (x.id, x))
        .collect();
    let episodes = db_context
        .episodes()
        .filter(
            domain::Episode::fields()
                .season_id()
                .in_list(seasons.keys().copied().collect::<Vec<_>>()),
        )
        .exec(db_context.executor_mut())
        .await?;
    fn show_episodes<'a>(
        seasons: &'a HashMap<Uuid, domain::Season>,
        episodes: &'a [domain::Episode],
    ) -> impl Iterator<Item = ShowEpisode<'a>> {
        episodes.iter().filter_map(|x| {
            Some(ShowEpisode {
                season: seasons.get(&x.season_id)?,
                episode: x,
                watch_state: None,
            })
        })
    }
    let Some(next) = domain::next_episode(
        show_episodes(&seasons, &episodes),
        show_episodes(&seasons, playing),
    ) else {
        return Ok(None);
    };
    let Some(media_file_id) = next.episode.media_file_id else {
        return Ok(None);
    };
    let thumb = db_context
        .artwork()
        .filter(
            Artwork::fields()
                .episode_id()
                .eq(next.episode.id)
                .and(Artwork::fields().kind().is_thumb()),
        )
        .first()
        .exec(db_context.executor_mut())
        .await?
        .map(|x| Image::new(&x, url_signer, now));
    Ok(Some(NextEpisode {
        id: next.episode.id,
        media_file_id,
        season_number: next.season.number,
        episode_number: next.episode.number,
        title: next.episode.title.clone(),
        thumb,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::EndedHlsOutputs;
    use crate::test::StartedHlsOutputs;
    use crate::test::add_season;
    use crate::test::at;
    use crate::test::episode;
    use crate::test::mock_db_context;
    use crate::test::movie;
    use crate::test::show;
    use crate::test::url_signer;
    use domain::ArtworkKind;
    use domain::AudioCodecSupport;
    use domain::AudioTranscode;
    use domain::AudioTranscodeCodec;
    use domain::HdrType;
    use domain::VideoCodecSupport;
    use domain::VideoStream;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    const NOW: &str = "2026-11-03T20:00:00Z";
    const MKV: &str = "matroska,webm";
    const MP4: &str = "mov,mp4,m4a,3gp,3g2,mj2";

    struct Audio {
        index: i64,
        codec: &'static str,
        channels: i64,
        language: &'static str,
        title: &'static str,
        default: bool,
    }

    const ENGLISH_STEREO: Audio = Audio {
        index: 1,
        codec: "aac",
        channels: 2,
        language: "eng",
        title: "Stereo",
        default: true,
    };

    const JAPANESE_STEREO: Audio = Audio {
        index: 2,
        language: "jpn",
        default: false,
        ..ENGLISH_STEREO
    };

    struct Subtitle {
        codec: &'static str,
        language: &'static str,
        forced: bool,
        hearing_impaired: bool,
    }

    const ENGLISH: Subtitle = Subtitle {
        codec: "subrip",
        language: "eng",
        forced: false,
        hearing_impaired: false,
    };

    struct Request {
        session_id: Uuid,
        client_kind: ClientKind,
        profile: CapabilityProfile,
        audio_track_id: Option<Uuid>,
        subtitle_track_id: Option<Option<Uuid>>,
        force_hls: bool,
    }

    struct Fixture {
        db_context: DbContext,
        user_id: Uuid,
        session_id: Uuid,
        started: StartedHlsOutputs,
        ended: EndedHlsOutputs,
        keyframes: Arc<Mutex<Vec<Duration>>>,
        keyframe_reads: Arc<AtomicUsize>,
    }

    impl Fixture {
        async fn new() -> Self {
            Self {
                db_context: mock_db_context().await,
                user_id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                started: StartedHlsOutputs::default(),
                ended: EndedHlsOutputs::default(),
                keyframes: Arc::new(Mutex::new(
                    (0..2000).map(|x| Duration::from_secs(x * 6)).collect(),
                )),
                keyframe_reads: Arc::default(),
            }
        }

        fn request(&self, profile: CapabilityProfile) -> Request {
            Request {
                session_id: self.session_id,
                client_kind: ClientKind::Web,
                profile,
                audio_track_id: None,
                subtitle_track_id: None,
                force_hls: false,
            }
        }

        async fn start(&self, media_file_id: Uuid) -> Playback {
            playback(
                self.start_with(media_file_id, self.request(browser()))
                    .await,
            )
        }

        async fn start_with(&self, media_file_id: Uuid, request: Request) -> Output {
            let keyframes = self.keyframes.clone();
            let keyframe_reads = self.keyframe_reads.clone();
            call(
                Dependencies {
                    db_context: self.db_context.clone(),
                    clock: Clock::new(|| at(NOW)),
                    url_signer: url_signer(),
                    read_keyframes: ReadKeyframes::new(move |_| {
                        keyframe_reads.fetch_add(1, Ordering::SeqCst);
                        let keyframes = keyframes.lock().unwrap().clone();
                        Box::pin(async move { Ok(keyframes) })
                    }),
                    start_hls_output: self.started.port(),
                    end_hls_output: self.ended.port(),
                },
                self.user_id,
                request.session_id,
                request.client_kind,
                media_file_id,
                0.0,
                request.audio_track_id,
                request.subtitle_track_id,
                &request.profile,
                request.force_hls,
            )
            .await
            .unwrap()
        }

        async fn add_media_file(&self, container: Option<&str>, audio: &[Audio]) -> MediaFile {
            let mut db_context = self.db_context.clone();
            let media_file = db_context
                .media_files()
                .create()
                .path(format!(
                    "/media/Movies/Arrival (2016)/{}.mkv",
                    Uuid::new_v4()
                ))
                .size(Some(1_000))
                .modified_at(Some(at("2026-10-29T09:00:00Z")))
                .container(container.map(str::to_owned))
                .duration_ms(Some(6_981_000))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            media_file
                .video_streams()
                .create()
                .index(0)
                .codec("h264")
                .profile(Some("High".to_owned()))
                .width(1920)
                .height(800)
                .hdr_type(HdrType::Sdr)
                .interlaced(false)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            for audio in audio {
                media_file
                    .audio_streams()
                    .create()
                    .index(audio.index)
                    .codec(audio.codec)
                    .language(Some(audio.language.to_owned()))
                    .channels(audio.channels)
                    .title(Some(audio.title.to_owned()))
                    .is_default(audio.default)
                    .exec(db_context.executor_mut())
                    .await
                    .unwrap();
            }
            media_file
        }

        async fn add_movie(&self, container: Option<&str>) -> MediaFile {
            let media_file = self
                .add_media_file(container, &[JAPANESE_STEREO, ENGLISH_STEREO])
                .await;
            self.add_movie_playing(media_file.id, true).await;
            media_file
        }

        async fn add_movie_playing(&self, media_file_id: Uuid, in_library: bool) -> Uuid {
            let mut db_context = self.db_context.clone();
            movie(&db_context, "Arrival")
                .media_file_id(Some(media_file_id))
                .in_library(in_library)
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .id
        }

        async fn add_subtitles(&self, media_file: &MediaFile, subtitles: &[Subtitle]) -> Vec<Uuid> {
            let mut db_context = self.db_context.clone();
            let mut ids = Vec::new();
            for (index, subtitle) in (3..).zip(subtitles) {
                ids.push(
                    media_file
                        .subtitle_streams()
                        .create()
                        .index(index)
                        .codec(subtitle.codec)
                        .language(Some(subtitle.language.to_owned()))
                        .is_default(false)
                        .forced(subtitle.forced)
                        .hearing_impaired(subtitle.hearing_impaired)
                        .text(subtitle.codec != "hdmv_pgs_subtitle")
                        .exec(db_context.executor_mut())
                        .await
                        .unwrap()
                        .id,
                );
            }
            ids
        }

        async fn add_subtitle_file(&self, media_file: &MediaFile, language: &str) -> Uuid {
            let mut db_context = self.db_context.clone();
            media_file
                .subtitle_files()
                .create()
                .path(media_file.path.replace(".mkv", &format!(".{language}.srt")))
                .language(Some(language.to_owned()))
                .hearing_impaired(false)
                .forced(false)
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .id
        }

        async fn set_video(&self, media_file_id: Uuid, codec: &str) {
            let mut db_context = self.db_context.clone();
            VideoStream::filter_by_media_file_id(media_file_id)
                .update()
                .codec(codec)
                .profile(Some("Main 10".to_owned()))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        async fn remember(&self, audio: Option<&str>, subtitle: Option<&str>) {
            let mut db_context = self.db_context.clone();
            db_context
                .playback_settings()
                .create()
                .user_id(self.user_id)
                .audio_language(audio.map(str::to_owned))
                .subtitle_language(subtitle.map(str::to_owned))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        async fn stored(&self, playback_id: Uuid) -> domain::Playback {
            let mut db_context = self.db_context.clone();
            db_context
                .playbacks()
                .find(playback_id)
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .unwrap()
        }
    }

    fn playback(output: Output) -> Playback {
        match output {
            Output::Playback(playback) => *playback,
            output => panic!("expected a playback, got {output:?}"),
        }
    }

    fn browser() -> CapabilityProfile {
        CapabilityProfile {
            containers: vec!["mp4".to_owned(), "webm".to_owned()],
            video_codecs: vec![VideoCodecSupport {
                codec: "h264".to_owned(),
                profiles: vec!["main".to_owned(), "high".to_owned()],
                max_width: 3840,
                max_height: 2160,
                hdr_types: vec![HdrType::Sdr],
            }],
            audio_codecs: vec![AudioCodecSupport {
                codec: "aac".to_owned(),
                max_channels: 2,
            }],
            subtitle_codecs: Vec::new(),
        }
    }

    fn apple_tv() -> CapabilityProfile {
        CapabilityProfile {
            containers: vec!["mp4".to_owned(), "mov".to_owned()],
            audio_codecs: [("aac", 2), ("ac3", 6), ("eac3", 8)]
                .map(|(codec, max_channels)| AudioCodecSupport {
                    codec: codec.to_owned(),
                    max_channels,
                })
                .into(),
            ..browser()
        }
    }

    fn track_ids(playback: &Playback) -> (Option<Uuid>, Option<Uuid>) {
        (playback.audio_track_id, playback.subtitle_track_id)
    }

    #[tokio::test]
    async fn a_file_the_profile_covers_plays_directly_with_its_movie_tracks_and_settings() {
        let fixture = Fixture::new().await;
        let media_file = fixture
            .add_media_file(Some(MP4), &[JAPANESE_STEREO, ENGLISH_STEREO])
            .await;
        let movie_id = fixture.add_movie_playing(media_file.id, true).await;
        let subtitle_file = fixture.add_subtitle_file(&media_file, "en").await;

        let playback = fixture.start(media_file.id).await;

        let english = playback.audio_tracks[0].id;
        assert_eq!(
            (playback.method, &playback.reasons, playback.duration),
            (
                PlayMethod::DirectPlay,
                &vec![],
                Some(Duration::from_secs(6981))
            )
        );
        assert_eq!(
            playback.movie.as_ref().map(|x| (x.id, x.title.as_str())),
            Some((movie_id, "Arrival"))
        );
        assert!(playback.episodes.is_empty());
        assert_eq!(track_ids(&playback), (Some(english), None));
        assert_eq!(
            playback
                .audio_tracks
                .iter()
                .map(|x| (x.language.as_deref(), x.default, x.in_stream))
                .collect::<Vec<_>>(),
            [(Some("eng"), true, true), (Some("jpn"), false, true)]
        );
        assert_eq!(
            playback
                .subtitle_tracks
                .iter()
                .map(|x| (x.id, x.codec.as_str(), x.external, x.in_stream))
                .collect::<Vec<_>>(),
            [(subtitle_file, "srt", true, false)]
        );
        assert_eq!(
            (
                playback.autoplay_next_episode,
                playback.ask_if_still_watching
            ),
            (true, false)
        );
        assert_eq!(fixture.started.count(), 0);
    }

    #[tokio::test]
    async fn a_two_episode_file_plays_as_both_episodes_in_order_with_the_next_episode_after_both() {
        let fixture = Fixture::new().await;
        let mut db_context = fixture.db_context.clone();
        let media_file = fixture.add_media_file(Some(MP4), &[ENGLISH_STEREO]).await;
        let next_media_file = fixture.add_media_file(Some(MP4), &[ENGLISH_STEREO]).await;
        let show_id = show(&db_context, "Severance")
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        let season_1 = add_season(&mut db_context, show_id, 1).await;
        let specials = add_season(&mut db_context, show_id, 0).await;
        let season_2 = add_season(&mut db_context, show_id, 2).await;
        let mut playing = Vec::new();
        for (number, title) in [(3, "In Perpetuity"), (2, "Half Loop")] {
            playing.push(
                episode(&db_context, season_1)
                    .number(number)
                    .title(title)
                    .media_file_id(Some(media_file.id))
                    .exec(db_context.executor_mut())
                    .await
                    .unwrap()
                    .id,
            );
        }
        for season_id in [specials, season_2] {
            episode(&db_context, season_id)
                .media_file_id(Some(next_media_file.id))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        let next = db_context
            .episodes()
            .filter(domain::Episode::fields().season_id().eq(season_2))
            .get(db_context.executor_mut())
            .await
            .unwrap();
        db_context
            .artwork()
            .create()
            .show_id(Some(show_id))
            .episode_id(Some(next.id))
            .kind(ArtworkKind::Thumb)
            .path("/media/TV/Severance/Season 02/Severance - S02E01-thumb.jpg")
            .modified_at(at(NOW))
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        let playback = fixture.start(media_file.id).await;

        assert!(playback.movie.is_none());
        assert_eq!(
            playback
                .episodes
                .iter()
                .map(|x| (
                    x.id,
                    x.show_title.as_str(),
                    x.season_number,
                    x.episode_number
                ))
                .collect::<Vec<_>>(),
            [
                (playing[1], "Severance", 1, 2),
                (playing[0], "Severance", 1, 3)
            ]
        );
        let next_episode = playback.next_episode.unwrap();
        assert_eq!(
            (
                next_episode.id,
                next_episode.media_file_id,
                next_episode.season_number,
                next_episode.episode_number,
                next_episode.thumb.is_some(),
            ),
            (next.id, next_media_file.id, 2, 1, true)
        );
    }

    #[tokio::test]
    async fn a_playback_carries_the_users_playback_settings() {
        let fixture = Fixture::new().await;
        let media_file = fixture.add_movie(Some(MP4)).await;
        let mut db_context = fixture.db_context.clone();
        db_context
            .playback_settings()
            .create()
            .user_id(fixture.user_id)
            .autoplay_next_episode(false)
            .ask_if_still_watching(true)
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        let playback = fixture.start(media_file.id).await;

        assert_eq!(
            (
                playback.autoplay_next_episode,
                playback.ask_if_still_watching
            ),
            (false, true)
        );
    }

    #[tokio::test]
    async fn a_file_whose_container_alone_is_wrong_remuxes_through_an_hls_output() {
        let fixture = Fixture::new().await;
        let media_file = fixture.add_movie(Some(MKV)).await;

        let playback = fixture.start(media_file.id).await;

        let output = fixture.started.last();
        assert_eq!(
            (playback.method, playback.reasons),
            (
                PlayMethod::Remux,
                vec![PlayReason::Container("mkv".to_owned())]
            )
        );
        assert_eq!(output.playback_id, playback.id);
        assert_eq!(output.video_transcode, None);
        assert_eq!(output.keyframes.len(), 2000);
        assert_eq!(
            output
                .audio
                .iter()
                .map(|x| (x.stream.index, x.transcode, x.selected))
                .collect::<Vec<_>>(),
            [(1, None, true), (2, None, false)]
        );
    }

    #[tokio::test]
    async fn an_hls_playback_carries_every_text_subtitle_in_its_stream_but_no_image_subtitle() {
        let fixture = Fixture::new().await;
        let media_file = fixture.add_movie(Some(MKV)).await;
        fixture
            .add_subtitles(
                &media_file,
                &[
                    ENGLISH,
                    Subtitle {
                        codec: "hdmv_pgs_subtitle",
                        ..ENGLISH
                    },
                ],
            )
            .await;
        fixture.add_subtitle_file(&media_file, "en").await;

        let playback = fixture.start(media_file.id).await;

        let output = fixture.started.last();
        assert_eq!(
            playback
                .subtitle_tracks
                .iter()
                .map(|x| (x.codec.as_str(), x.in_stream))
                .collect::<Vec<_>>(),
            [
                ("subrip", true),
                ("hdmv_pgs_subtitle", false),
                ("srt", true)
            ]
        );
        assert_eq!(
            output
                .subtitle_streams
                .iter()
                .map(|x| x.codec.as_str())
                .collect::<Vec<_>>(),
            ["subrip"]
        );
        assert_eq!(output.subtitle_files.len(), 1);
    }

    #[tokio::test]
    async fn a_tv_session_remuxes_a_file_whose_text_subtitle_sits_beside_it() {
        let fixture = Fixture::new().await;
        let media_file = fixture.add_movie(Some(MP4)).await;
        fixture.add_subtitle_file(&media_file, "en").await;

        let playback = playback(
            fixture
                .start_with(
                    media_file.id,
                    Request {
                        client_kind: ClientKind::AppleTv,
                        ..fixture.request(apple_tv())
                    },
                )
                .await,
        );

        assert_eq!(
            (playback.method, playback.reasons),
            (
                PlayMethod::Remux,
                vec![PlayReason::Subtitle("srt".to_owned())]
            )
        );
    }

    #[tokio::test]
    async fn force_hls_remuxes_a_file_the_profile_plays_directly_with_a_forced_hls_reason() {
        let fixture = Fixture::new().await;
        let media_file = fixture.add_movie(Some(MP4)).await;

        let playback = playback(
            fixture
                .start_with(
                    media_file.id,
                    Request {
                        force_hls: true,
                        ..fixture.request(browser())
                    },
                )
                .await,
        );

        assert_eq!(
            (playback.method, playback.reasons),
            (PlayMethod::Remux, vec![PlayReason::ForcedHls])
        );
        assert_eq!(fixture.started.last().playback_id, playback.id);
    }

    #[tokio::test]
    async fn a_video_codec_the_profile_does_not_list_transcodes_the_video_without_reading_keyframes()
     {
        let fixture = Fixture::new().await;
        let media_file = fixture.add_movie(Some(MKV)).await;
        fixture.set_video(media_file.id, "hevc").await;

        let playback = fixture.start(media_file.id).await;

        let output = fixture.started.last();
        assert_eq!(
            (playback.method, playback.reasons),
            (
                PlayMethod::Transcode,
                vec![
                    PlayReason::Container("mkv".to_owned()),
                    PlayReason::VideoCodec("hevc".to_owned()),
                ]
            )
        );
        assert!(output.video_transcode.is_some());
        assert_eq!(fixture.keyframe_reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_file_without_a_usable_keyframe_index_transcodes_the_video_with_a_no_keyframe_index_reason()
     {
        let fixture = Fixture::new().await;
        *fixture.keyframes.lock().unwrap() = vec![Duration::ZERO, Duration::from_secs(30)];
        let media_file = fixture.add_movie(Some(MKV)).await;

        let playback = fixture.start(media_file.id).await;

        assert_eq!(
            (playback.method, playback.reasons),
            (
                PlayMethod::Transcode,
                vec![
                    PlayReason::Container("mkv".to_owned()),
                    PlayReason::NoKeyframeIndex,
                ]
            )
        );
        assert!(fixture.started.last().video_transcode.is_some());
    }

    #[tokio::test]
    async fn audio_the_profile_does_not_cover_is_transcoded_in_the_hls_output_and_the_rest_copied()
    {
        let fixture = Fixture::new().await;
        let media_file = fixture
            .add_media_file(
                Some(MKV),
                &[
                    Audio {
                        codec: "dts",
                        channels: 6,
                        title: "DTS-HD MA 5.1",
                        ..ENGLISH_STEREO
                    },
                    Audio {
                        index: 2,
                        codec: "dts",
                        title: "Commentary",
                        default: false,
                        ..ENGLISH_STEREO
                    },
                    Audio {
                        index: 3,
                        language: "fre",
                        default: false,
                        ..ENGLISH_STEREO
                    },
                ],
            )
            .await;
        fixture.add_movie_playing(media_file.id, true).await;

        let playback = playback(
            fixture
                .start_with(media_file.id, fixture.request(apple_tv()))
                .await,
        );

        let eac3 = |channels| {
            Some(AudioTranscode {
                codec: AudioTranscodeCodec::Eac3,
                channels,
            })
        };
        assert_eq!(
            (playback.method, playback.reasons),
            (
                PlayMethod::Transcode,
                vec![
                    PlayReason::Container("mkv".to_owned()),
                    PlayReason::AudioCodec("dts".to_owned()),
                ]
            )
        );
        assert_eq!(
            fixture
                .started
                .last()
                .audio
                .iter()
                .map(|x| (x.stream.index, x.transcode, x.selected))
                .collect::<Vec<_>>(),
            [(1, eac3(6), true), (2, eac3(2), false), (3, None, false)]
        );
    }

    #[tokio::test]
    async fn the_keyframes_are_read_on_the_first_remux_and_again_once_the_file_changes() {
        let fixture = Fixture::new().await;
        let media_file = fixture.add_movie(Some(MKV)).await;

        fixture.start(media_file.id).await;
        fixture.start(media_file.id).await;
        let before_change = fixture.keyframe_reads.load(Ordering::SeqCst);
        let mut db_context = fixture.db_context.clone();
        MediaFile::filter_by_id(media_file.id)
            .update()
            .size(Some(1_001))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let playback = fixture.start(media_file.id).await;

        assert_eq!(before_change, 1);
        assert_eq!(fixture.keyframe_reads.load(Ordering::SeqCst), 2);
        assert_eq!(playback.method, PlayMethod::Remux);
    }

    #[tokio::test]
    async fn a_new_playback_ends_the_open_playback_of_the_same_session_and_its_hls_output() {
        let fixture = Fixture::new().await;
        let media_file = fixture.add_movie(Some(MP4)).await;
        let first = fixture.start(media_file.id).await;
        let other_session = playback(
            fixture
                .start_with(
                    media_file.id,
                    Request {
                        session_id: Uuid::new_v4(),
                        ..fixture.request(browser())
                    },
                )
                .await,
        );

        let second = fixture.start(media_file.id).await;

        assert_eq!(fixture.stored(first.id).await.ended_at, Some(at(NOW)));
        assert_eq!(fixture.stored(second.id).await.ended_at, None);
        assert_eq!(fixture.stored(other_session.id).await.ended_at, None);
        assert_eq!(fixture.ended.playback_ids(), [first.id]);
    }

    #[tokio::test]
    async fn without_remembered_languages_a_playback_starts_with_the_files_default_audio_and_no_subtitles()
     {
        let fixture = Fixture::new().await;
        let media_file = fixture.add_movie(Some(MKV)).await;
        fixture.add_subtitles(&media_file, &[ENGLISH]).await;

        let playback = fixture.start(media_file.id).await;

        assert_eq!(
            track_ids(&playback),
            (Some(playback.audio_tracks[0].id), None)
        );
        assert_eq!(playback.audio_tracks[0].language.as_deref(), Some("eng"));
        assert_eq!(fixture.started.last().selected_subtitle, None);
    }

    #[tokio::test]
    async fn a_playback_starts_with_the_first_audio_track_in_the_remembered_audio_language() {
        let fixture = Fixture::new().await;
        fixture.remember(Some("jpn"), None).await;
        let media_file = fixture
            .add_media_file(
                Some(MKV),
                &[
                    ENGLISH_STEREO,
                    Audio {
                        language: "ja",
                        ..JAPANESE_STEREO
                    },
                    Audio {
                        index: 3,
                        title: "Commentary",
                        ..JAPANESE_STEREO
                    },
                ],
            )
            .await;
        fixture.add_movie_playing(media_file.id, true).await;

        let playback = fixture.start(media_file.id).await;

        assert_eq!(playback.audio_track_id, Some(playback.audio_tracks[1].id));
        assert_eq!(
            fixture
                .started
                .last()
                .audio
                .iter()
                .map(|x| x.selected)
                .collect::<Vec<_>>(),
            [false, true, false]
        );
    }

    #[tokio::test]
    async fn a_playback_starts_with_a_regular_subtitle_in_the_remembered_subtitle_language() {
        let fixture = Fixture::new().await;
        fixture.remember(None, Some("eng")).await;
        let media_file = fixture.add_movie(Some(MKV)).await;
        let ids = fixture
            .add_subtitles(
                &media_file,
                &[
                    Subtitle {
                        forced: true,
                        ..ENGLISH
                    },
                    Subtitle {
                        codec: "hdmv_pgs_subtitle",
                        ..ENGLISH
                    },
                    Subtitle {
                        hearing_impaired: true,
                        ..ENGLISH
                    },
                    ENGLISH,
                ],
            )
            .await;

        let playback = fixture.start(media_file.id).await;

        assert_eq!(playback.subtitle_track_id, Some(ids[3]));
        assert_eq!(fixture.started.last().selected_subtitle, Some(2));
    }

    #[tokio::test]
    async fn a_play_request_may_name_the_audio_and_subtitle_tracks_or_turn_subtitles_off() {
        let fixture = Fixture::new().await;
        fixture.remember(Some("eng"), Some("eng")).await;
        let media_file = fixture.add_movie(Some(MKV)).await;
        let ids = fixture
            .add_subtitles(&media_file, &[ENGLISH, ENGLISH])
            .await;
        let defaults = fixture.start(media_file.id).await;
        let japanese = defaults.audio_tracks[1].id;

        let named = playback(
            fixture
                .start_with(
                    media_file.id,
                    Request {
                        audio_track_id: Some(japanese),
                        subtitle_track_id: Some(Some(ids[1])),
                        ..fixture.request(browser())
                    },
                )
                .await,
        );
        let off = playback(
            fixture
                .start_with(
                    media_file.id,
                    Request {
                        subtitle_track_id: Some(None),
                        ..fixture.request(browser())
                    },
                )
                .await,
        );

        assert_eq!(defaults.subtitle_track_id, Some(ids[0]));
        assert_eq!(track_ids(&named), (Some(japanese), Some(ids[1])));
        assert_eq!(off.subtitle_track_id, None);
    }

    #[tokio::test]
    async fn the_playback_stores_the_default_tracks_until_its_first_report() {
        let fixture = Fixture::new().await;
        let media_file = fixture.add_movie(Some(MKV)).await;
        let ids = fixture.add_subtitles(&media_file, &[ENGLISH]).await;
        let defaults = fixture.start(media_file.id).await;

        let named = playback(
            fixture
                .start_with(
                    media_file.id,
                    Request {
                        audio_track_id: Some(defaults.audio_tracks[1].id),
                        subtitle_track_id: Some(Some(ids[0])),
                        ..fixture.request(browser())
                    },
                )
                .await,
        );

        let stored = fixture.stored(named.id).await;
        assert_eq!(
            (stored.audio_track_id, stored.subtitle_track_id),
            (defaults.audio_track_id, None)
        );
    }

    #[tokio::test]
    async fn an_unknown_or_hidden_media_file_is_not_found() {
        let fixture = Fixture::new().await;
        let hidden = fixture.add_media_file(Some(MP4), &[ENGLISH_STEREO]).await;
        fixture.add_movie_playing(hidden.id, false).await;

        for media_file_id in [hidden.id, Uuid::new_v4()] {
            let output = fixture
                .start_with(media_file_id, fixture.request(browser()))
                .await;

            assert!(matches!(output, Output::NotFound), "{output:?}");
        }
    }

    #[tokio::test]
    async fn a_media_file_whose_container_sync_could_not_read_is_unreadable() {
        let fixture = Fixture::new().await;
        let media_file = fixture.add_movie(None).await;

        let output = fixture
            .start_with(media_file.id, fixture.request(browser()))
            .await;

        assert!(matches!(output, Output::UnreadableMediaFile), "{output:?}");
    }
}
