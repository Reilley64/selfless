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
use crate::ports::persistence::DbScope;
use crate::telemetry::attribute_values::client_kind;
use crate::telemetry::attribute_values::play_method;
use crate::telemetry::attribute_values::reason_kinds;
use domain::AudioStream;
use domain::CapabilityProfile;
use domain::ClientKind;
use domain::MediaFile;
use domain::PlayMethod;
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

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    session_id: Uuid,
    session_client_kind: ClientKind,
    media_file_id: Uuid,
    start_position: f64,
    profile: &CapabilityProfile,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        url_signer,
    } = dependencies.into();
    let Some(media_file) = db_context
        .media_files()
        .find(media_file_id)
        .include(MediaFile::fields().video_streams())
        .include(MediaFile::fields().audio_streams())
        .include(MediaFile::fields().subtitle_streams())
        .include(MediaFile::fields().subtitle_files())
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
    let video_streams = media_file.video_streams.into_inner();
    let mut audio_streams = media_file.audio_streams.into_inner();
    let mut subtitle_streams = media_file.subtitle_streams.into_inner();
    let mut subtitle_files = media_file.subtitle_files.into_inner();
    let reasons = profile.direct_play_reasons(
        &container,
        video_streams.iter().min_by_key(|x| x.index),
        AudioStream::selected_by_default(&audio_streams),
    );
    if !reasons.is_empty() {
        Span::current().set_attribute(SELFLESS_PLAYBACK_REASON_KINDS, reason_kinds(&reasons));
        return Ok(Output::NeedsHls);
    }
    let episodes = player_episodes(&mut db_context, episodes)
        .await
        .map_err(Error::caused_by)?;
    let now = clock.call();
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
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
    }
    let playback = transaction
        .playbacks()
        .create()
        .user_id(user_id)
        .session_id(session_id)
        .media_file_id(media_file_id)
        .method(PlayMethod::DirectPlay)
        .position(start_position)
        .paused(false)
        .started_at(now)
        .last_reported_at(now)
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
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
    audio_streams.sort_by_key(|x| x.index);
    subtitle_streams.sort_by_key(|x| x.index);
    subtitle_files.sort_by(|a, b| a.path.cmp(&b.path));
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
                in_stream: profile.subtitle_codecs.contains(&x.codec),
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
                in_stream: false,
            }))
            .collect(),
    })))
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
