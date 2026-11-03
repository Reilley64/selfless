mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::AudioTrack;
pub use output::Episode;
pub use output::Movie;
pub use output::Output;
pub use output::Playback;
pub use output::SubtitleTrack;

use crate::attributes::SELFLESS_AUTH_CLIENT_KIND;
use crate::attributes::SELFLESS_PLAYBACK_METHOD;
use crate::attributes::SELFLESS_PLAYBACK_REASON_KINDS;
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
use domain::AudioStream;
use domain::CapabilityProfile;
use domain::ClientKind;
use domain::KeyframeIndex;
use domain::MediaFile;
use domain::PlayMethod;
use domain::PlayReason;
use domain::usable_keyframe_index;
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
    let audio = AudioStream::selected_by_default(media_file.audio_streams.get());
    let mut reasons = profile.direct_play_reasons(&container, video, audio);
    if force_hls && reasons.is_empty() {
        reasons.push(PlayReason::ForcedHls);
    }
    let copied_video = match (video, duration) {
        (Some(video), Some(duration))
            if !reasons.is_empty() && reasons.iter().all(PlayReason::copies_video) =>
        {
            let keyframes = keyframes(&mut db_context, &read_keyframes, &media_file).await?;
            if usable_keyframe_index(&keyframes) {
                Some((video, duration, keyframes))
            } else {
                reasons.push(PlayReason::NoKeyframeIndex);
                None
            }
        }
        _ => None,
    };
    let mut audio_tracks = Vec::new();
    if copied_video.is_some() {
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
                selected: audio.is_some_and(|x| x.id == stream.id),
            });
        }
    }
    if !reasons.is_empty() {
        Span::current().set_attribute(SELFLESS_PLAYBACK_REASON_KINDS, reason_kinds(&reasons));
        if copied_video.is_none() {
            return Ok(Output::NeedsTranscode);
        }
    }
    let method = if copied_video.is_none() {
        PlayMethod::DirectPlay
    } else if audio_tracks.iter().any(|x| x.transcode.is_some()) {
        PlayMethod::Transcode
    } else {
        PlayMethod::Remux
    };
    let episodes = player_episodes(&mut db_context, episodes)
        .await
        .map_err(Error::caused_by)?;
    let mut subtitle_streams = media_file.subtitle_streams.into_inner();
    let mut subtitle_files = media_file.subtitle_files.into_inner();
    subtitle_streams.sort_by_key(|x| x.index);
    subtitle_files.sort_by(|a, b| a.path.cmp(&b.path));
    let now = clock.call();
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
        .started_at(now)
        .last_reported_at(now)
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    for id in ended {
        end_hls_session.call(id).await;
    }
    if let Some((video, duration, keyframes)) = copied_video {
        start_hls_session
            .call(HlsSession {
                playback_id: playback.id,
                path: media_file.path.clone(),
                duration,
                keyframes,
                video: ports::VideoStream {
                    index: video.index,
                    codec: video.codec.clone(),
                    profile: video.profile.clone(),
                    width: video.width,
                    height: video.height,
                    frame_rate: video.frame_rate,
                    hdr_type: video.hdr_type,
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
