use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use domain::EpisodeWatchState;
use domain::MediaFile;
use domain::Movie;
use domain::MovieWatchState;
use domain::WatchState;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Debug, Default)]
pub(crate) struct MovieWatch {
    pub(crate) state: WatchState,
    pub(crate) progress: Option<f64>,
}

pub(crate) async fn movie_watches<'a>(
    db_context: &mut DbContext,
    user_id: Uuid,
    movies: impl IntoIterator<Item = &'a Movie>,
) -> toasty::Result<HashMap<Uuid, MovieWatch>> {
    let movies: Vec<&Movie> = movies.into_iter().collect();
    let states: Vec<_> = db_context
        .movie_watch_states()
        .filter(
            MovieWatchState::fields().user_id().eq(user_id).and(
                MovieWatchState::fields()
                    .movie_id()
                    .in_list(movies.iter().map(|x| x.id).collect::<Vec<_>>()),
            ),
        )
        .exec(db_context.executor_mut())
        .await?
        .iter()
        .map(|x| (x.movie_id, x.watch_state()))
        .collect();
    let media_file_of: HashMap<Uuid, Uuid> = movies
        .iter()
        .filter_map(|x| Some((x.id, x.media_file_id?)))
        .collect();
    let resuming: Vec<Uuid> = states
        .iter()
        .filter(|(_, state)| state.resume_position_seconds.is_some())
        .filter_map(|(movie_id, _)| media_file_of.get(movie_id).copied())
        .collect();
    let durations: HashMap<Uuid, Option<i64>> = if resuming.is_empty() {
        HashMap::new()
    } else {
        db_context
            .media_files()
            .filter(MediaFile::fields().id().in_list(resuming))
            .exec(db_context.executor_mut())
            .await?
            .into_iter()
            .map(|x| (x.id, x.duration_ms))
            .collect()
    };
    Ok(states
        .into_iter()
        .map(|(movie_id, state)| {
            let duration_ms = media_file_of
                .get(&movie_id)
                .and_then(|x| durations.get(x).copied().flatten());
            let progress = state.progress(duration_ms);
            (movie_id, MovieWatch { state, progress })
        })
        .collect())
}

pub(crate) async fn episode_watch_states(
    db_context: &mut DbContext,
    user_id: Uuid,
    episode_ids: Vec<Uuid>,
) -> toasty::Result<HashMap<Uuid, EpisodeWatchState>> {
    Ok(db_context
        .episode_watch_states()
        .filter(
            EpisodeWatchState::fields().user_id().eq(user_id).and(
                EpisodeWatchState::fields()
                    .episode_id()
                    .in_list(episode_ids),
            ),
        )
        .exec(db_context.executor_mut())
        .await?
        .into_iter()
        .map(|x| (x.episode_id, x))
        .collect())
}
