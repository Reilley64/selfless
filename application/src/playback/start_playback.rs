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
use crate::ports::HlsSession;
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
        start_hls_session,
        end_hls_session,
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
        end_hls_session.call(id).await;
    }
    if let Some((video, duration, keyframes, video_transcode)) = hls {
        let selected_subtitle = subtitle_streams
            .iter()
            .filter(|x| x.text)
            .map(|x| x.id)
            .chain(subtitle_files.iter().map(|x| x.id))
            .position(|x| Some(x) == subtitle_track_id);
        start_hls_session
            .call(HlsSession {
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
