mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::attributes::SELFLESS_WATCH_PROGRESS_OUTCOME;
use crate::ports::persistence::DbScope;
use crate::spans::SELFLESS_WATCH_PROGRESS_RECORD;
use crate::telemetry::attribute_values::watch_progress_outcome;
use domain::EpisodeWatchState;
use domain::MediaFile;
use domain::MovieWatchState;
use domain::ReportOutcome;
use jiff::SignedDuration;
use std::collections::HashMap;
use tracing::Instrument;
use tracing::field;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    media_file_id: Uuid,
    position: SignedDuration,
    duration: SignedDuration,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let span = tracing::info_span!(
        "record_progress",
        otel.name = SELFLESS_WATCH_PROGRESS_RECORD,
        otel.kind = "internal",
        { SELFLESS_WATCH_PROGRESS_OUTCOME } = field::Empty,
    );
    let output = async {
        let reported_at = clock.call();
        let Some(media_file) = db_context
            .media_files()
            .find(media_file_id)
            .include(MediaFile::fields().movies())
            .include(MediaFile::fields().episodes())
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
        else {
            return Ok(Output {
                outcome: ReportOutcome::Nothing,
            });
        };
        let movie_ids = media_file
            .movies
            .get()
            .iter()
            .map(|x| x.id)
            .collect::<Vec<_>>();
        let episode_ids = media_file
            .episodes
            .get()
            .iter()
            .map(|x| x.id)
            .collect::<Vec<_>>();
        let outcome = if movie_ids.is_empty() && episode_ids.is_empty() {
            ReportOutcome::Nothing
        } else {
            ReportOutcome::of(position, duration)
        };
        let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
        let movie_states = transaction
            .movie_watch_states()
            .filter(
                MovieWatchState::fields().user_id().eq(user_id).and(
                    MovieWatchState::fields()
                        .movie_id()
                        .in_list(movie_ids.clone()),
                ),
            )
            .exec(transaction.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| (x.movie_id, x.watch_state()))
            .collect::<HashMap<_, _>>();
        for movie_id in movie_ids {
            let Some(state) = movie_states
                .get(&movie_id)
                .cloned()
                .unwrap_or_default()
                .after_report(position, duration, reported_at)
            else {
                continue;
            };
            MovieWatchState::upsert_by_user_id_and_movie_id(user_id, movie_id)
                .resume_position_seconds(state.resume_position_seconds)
                .watched(state.watched)
                .watched_at(state.watched_at)
                .last_played(state.last_played)
                .exec(transaction.executor_mut())
                .await
                .map_err(Error::caused_by)?;
        }
        let episode_states = transaction
            .episode_watch_states()
            .filter(
                EpisodeWatchState::fields().user_id().eq(user_id).and(
                    EpisodeWatchState::fields()
                        .episode_id()
                        .in_list(episode_ids.clone()),
                ),
            )
            .exec(transaction.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| (x.episode_id, x.watch_state()))
            .collect::<HashMap<_, _>>();
        for episode_id in episode_ids {
            let Some(state) = episode_states
                .get(&episode_id)
                .cloned()
                .unwrap_or_default()
                .after_report(position, duration, reported_at)
            else {
                continue;
            };
            EpisodeWatchState::upsert_by_user_id_and_episode_id(user_id, episode_id)
                .resume_position_seconds(state.resume_position_seconds)
                .watched(state.watched)
                .watched_at(state.watched_at)
                .last_played(state.last_played)
                .exec(transaction.executor_mut())
                .await
                .map_err(Error::caused_by)?;
        }
        transaction.commit().await.map_err(Error::caused_by)?;
        Ok::<_, Error>(Output { outcome })
    }
    .instrument(span.clone())
    .await?;
    span.record(
        SELFLESS_WATCH_PROGRESS_OUTCOME,
        watch_progress_outcome(output.outcome),
    );
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::ports::persistence::DbScope;
    use crate::test::add_season;
    use crate::test::at;
    use crate::test::episode;
    use crate::test::mock_db_context;
    use crate::test::movie;
    use crate::test::show;
    use domain::WatchState;
    use jiff::Timestamp;
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::trace::InMemorySpanExporter;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use test_fixtures::FakeClock;
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::layer::SubscriberExt;

    const EARLIER: &str = "2026-10-01T20:00:00Z";
    const REPORTED_AT: &str = "2026-11-03T21:00:00Z";
    const DURATION: SignedDuration = SignedDuration::from_secs(100 * 60);

    fn minutes(minutes: i64) -> SignedDuration {
        SignedDuration::from_mins(minutes)
    }

    struct Household {
        db_context: DbContext,
        clock: FakeClock,
        user_id: Uuid,
    }

    impl Household {
        async fn new() -> Self {
            let clock = FakeClock::new();
            clock.set(at(REPORTED_AT));
            Self {
                db_context: mock_db_context().await,
                clock,
                user_id: Uuid::new_v4(),
            }
        }

        async fn add_media_file(&self) -> Uuid {
            let mut db_context = self.db_context.clone();
            db_context
                .media_files()
                .create()
                .path(format!("/media/{}.mkv", Uuid::new_v4()))
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .id
        }

        async fn add_movie(&self, media_file_id: Uuid) -> Uuid {
            let mut db_context = self.db_context.clone();
            movie(&db_context, "Arrival")
                .media_file_id(Some(media_file_id))
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .id
        }

        async fn add_episodes<const N: usize>(
            &self,
            media_file_id: Uuid,
            numbers: &[i64; N],
            in_library: bool,
        ) -> [Uuid; N] {
            let mut db_context = self.db_context.clone();
            let severance = show(&db_context, "Severance")
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            let season_id = add_season(&mut db_context, severance.id, 1).await;
            let mut ids = [Uuid::nil(); N];
            for (id, &number) in ids.iter_mut().zip(numbers) {
                *id = episode(&db_context, season_id)
                    .number(number)
                    .media_file_id(Some(media_file_id))
                    .in_library(in_library)
                    .exec(db_context.executor_mut())
                    .await
                    .unwrap()
                    .id;
            }
            ids
        }

        async fn report(&self, media_file_id: Uuid, position: SignedDuration) -> ReportOutcome {
            call(
                Dependencies {
                    db_context: self.db_context.clone(),
                    clock: Clock::new(self.clock.reader()),
                },
                self.user_id,
                media_file_id,
                position,
                DURATION,
            )
            .await
            .unwrap()
            .outcome
        }

        async fn episode_watch_states(&self) -> Vec<(Uuid, Uuid, WatchState)> {
            let mut db_context = self.db_context.clone();
            let mut states = db_context
                .episode_watch_states()
                .all()
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .iter()
                .map(|x| (x.user_id, x.episode_id, x.watch_state()))
                .collect::<Vec<_>>();
            states.sort_by_key(|x| x.1);
            states
        }

        async fn movie_watch_states(&self) -> Vec<(Uuid, Uuid, WatchState)> {
            let mut db_context = self.db_context.clone();
            db_context
                .movie_watch_states()
                .all()
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .iter()
                .map(|x| (x.user_id, x.movie_id, x.watch_state()))
                .collect()
        }
    }

    #[tokio::test]
    async fn a_report_past_the_first_minute_saves_the_movies_resume_position() {
        let household = Household::new().await;
        let media_file = household.add_media_file().await;
        let arrival = household.add_movie(media_file).await;

        let outcome = household.report(media_file, minutes(20)).await;

        assert_eq!(outcome, ReportOutcome::ResumePosition);
        assert_eq!(
            household.movie_watch_states().await,
            [(
                household.user_id,
                arrival,
                WatchState {
                    resume_position_seconds: Some(1200),
                    watched: false,
                    watched_at: None,
                    last_played: Some(at(REPORTED_AT)),
                }
            )]
        );
    }
    #[tokio::test]
    async fn seeking_back_replaces_the_saved_resume_position() {
        let household = Household::new().await;
        let media_file = household.add_media_file().await;
        household.add_movie(media_file).await;
        household.report(media_file, minutes(50)).await;

        household.report(media_file, minutes(10)).await;

        let states = household.movie_watch_states().await;
        assert_eq!(
            states
                .iter()
                .map(|(_, _, x)| x.resume_position_seconds)
                .collect::<Vec<_>>(),
            [Some(600)]
        );
    }

    #[tokio::test]
    async fn a_short_sample_keeps_the_resume_position_and_last_played() {
        let household = Household::new().await;
        let media_file = household.add_media_file().await;
        household.add_movie(media_file).await;
        household.clock.set(at(EARLIER));
        household.report(media_file, minutes(40)).await;
        let before = household.movie_watch_states().await;
        household.clock.set(at(REPORTED_AT));

        let outcome = household
            .report(media_file, SignedDuration::from_secs(30))
            .await;

        assert_eq!(outcome, ReportOutcome::Nothing);
        assert_eq!(household.movie_watch_states().await, before);
    }

    #[tokio::test]
    async fn rewatching_a_watched_movie_keeps_its_watched_at() {
        let household = Household::new().await;
        let media_file = household.add_media_file().await;
        household.add_movie(media_file).await;
        household.clock.set(at(EARLIER));
        household.report(media_file, minutes(95)).await;
        household.clock.set(at(REPORTED_AT));

        household.report(media_file, minutes(10)).await;
        let rewatching = household.movie_watch_states().await[0].2.clone();
        let outcome = household.report(media_file, minutes(90)).await;

        assert_eq!(
            rewatching,
            WatchState {
                resume_position_seconds: Some(600),
                watched: true,
                watched_at: Some(at(EARLIER)),
                last_played: Some(at(REPORTED_AT)),
            }
        );
        assert_eq!(outcome, ReportOutcome::Watched);
        assert_eq!(
            household.movie_watch_states().await[0].2,
            WatchState {
                resume_position_seconds: None,
                watched: true,
                watched_at: Some(at(EARLIER)),
                last_played: Some(at(REPORTED_AT)),
            }
        );
    }

    #[tokio::test]
    async fn another_users_report_leaves_a_users_watch_state_alone() {
        let household = Household::new().await;
        let media_file = household.add_media_file().await;
        household.add_movie(media_file).await;
        household.report(media_file, minutes(40)).await;
        let noor = household.user_id;
        let ravi = Household {
            user_id: Uuid::new_v4(),
            db_context: household.db_context.clone(),
            clock: household.clock.clone(),
        };

        ravi.report(media_file, minutes(10)).await;

        let mut positions = household
            .movie_watch_states()
            .await
            .into_iter()
            .map(|(user_id, _, x)| (user_id, x.resume_position_seconds))
            .collect::<Vec<_>>();
        positions.sort();
        let mut expected = vec![(noor, Some(2400)), (ravi.user_id, Some(600))];
        expected.sort();
        assert_eq!(positions, expected);
    }
    #[tokio::test]
    async fn both_episodes_of_a_two_episode_file_get_the_same_watch_state() {
        let household = Household::new().await;
        let media_file = household.add_media_file().await;
        let mut episodes = household.add_episodes(media_file, &[2, 3], true).await;
        episodes.sort();

        household.report(media_file, minutes(20)).await;
        let resuming = household.episode_watch_states().await;
        let outcome = household.report(media_file, minutes(90)).await;

        let state = |resume_position_seconds, watched_at: Option<Timestamp>| WatchState {
            resume_position_seconds,
            watched: watched_at.is_some(),
            watched_at,
            last_played: Some(at(REPORTED_AT)),
        };
        assert_eq!(
            resuming,
            episodes.map(|x| (household.user_id, x, state(Some(1200), None)))
        );
        assert_eq!(outcome, ReportOutcome::Watched);
        assert_eq!(
            household.episode_watch_states().await,
            episodes.map(|x| (household.user_id, x, state(None, Some(at(REPORTED_AT)))))
        );
    }

    #[tokio::test]
    async fn rewatching_a_watched_episode_keeps_its_watched_at() {
        let household = Household::new().await;
        let media_file = household.add_media_file().await;
        household.add_episodes(media_file, &[3], true).await;
        household.clock.set(at(EARLIER));
        household.report(media_file, minutes(95)).await;
        household.clock.set(at(REPORTED_AT));

        household.report(media_file, minutes(30)).await;
        household.report(media_file, minutes(95)).await;

        let states = household.episode_watch_states().await;
        assert_eq!(states[0].2.watched_at, Some(at(EARLIER)));
        assert_eq!(states[0].2.last_played, Some(at(REPORTED_AT)));
    }

    #[tokio::test]
    async fn an_episode_that_left_the_library_is_written_as_usual() {
        let household = Household::new().await;
        let media_file = household.add_media_file().await;
        household.add_episodes(media_file, &[1], false).await;

        let outcome = household.report(media_file, minutes(20)).await;

        assert_eq!(outcome, ReportOutcome::ResumePosition);
        assert_eq!(
            household.episode_watch_states().await[0]
                .2
                .resume_position_seconds,
            Some(1200)
        );
    }
    #[tokio::test]
    async fn a_report_on_a_media_file_whose_items_were_removed_writes_nothing() {
        let household = Household::new().await;
        let media_file = household.add_media_file().await;

        let without_items = household.report(media_file, minutes(20)).await;
        let unknown = household.report(Uuid::new_v4(), minutes(20)).await;

        assert_eq!(
            (without_items, unknown),
            (ReportOutcome::Nothing, ReportOutcome::Nothing)
        );
        assert_eq!(household.movie_watch_states().await, []);
        assert_eq!(household.episode_watch_states().await, []);
    }
    #[tokio::test]
    async fn recording_a_report_is_traced_with_what_it_changed() {
        let household = Household::new().await;
        let media_file = household.add_media_file().await;
        household.add_movie(media_file).await;
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let _tracing = test_fixtures::set_default_subscriber(
            tracing_subscriber::registry()
                .with(LevelFilter::INFO)
                .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("selfless"))),
        );

        for position in [SignedDuration::from_secs(30), minutes(20), minutes(95)] {
            household.report(media_file, position).await;
        }

        let outcomes = exporter
            .get_finished_spans()
            .unwrap()
            .into_iter()
            .filter(|x| x.name == "selfless.watch_progress.record")
            .map(|x| {
                x.attributes
                    .into_iter()
                    .find(|x| x.key.as_str() == "selfless.watch_progress.outcome")
                    .map(|x| x.value.to_string())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes,
            ["none", "resume_position", "watched"].map(|x| Some(x.to_owned()))
        );
    }
}
