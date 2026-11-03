mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Episode;
pub use output::Output;

use crate::browsing::Image;
use crate::browsing::episode_watch_states;
use crate::ports::persistence::DbScope;
use domain::Artwork;
use domain::MediaFile;
use std::collections::HashMap;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    season_id: Uuid,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        url_signer,
    } = dependencies.into();
    let mut episodes = db_context
        .episodes()
        .filter(
            domain::Episode::fields()
                .season_id()
                .eq(season_id)
                .and(domain::Episode::in_library_filter()),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    if episodes.is_empty() {
        return Ok(Output::NotFound);
    }
    episodes.sort_by_key(|x| x.number);
    let media_files: HashMap<Uuid, MediaFile> = db_context
        .media_files()
        .filter(
            MediaFile::fields().id().in_list(
                episodes
                    .iter()
                    .filter_map(|x| x.media_file_id)
                    .collect::<Vec<_>>(),
            ),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| (x.id, x))
        .collect();
    let states = episode_watch_states(
        &mut db_context,
        user_id,
        episodes.iter().map(|x| x.id).collect(),
    )
    .await
    .map_err(Error::caused_by)?;
    let now = clock.call();
    let thumbs: HashMap<Uuid, Image> = db_context
        .artwork()
        .filter(
            Artwork::fields()
                .episode_id()
                .in_list(episodes.iter().map(|x| x.id).collect::<Vec<_>>())
                .and(Artwork::fields().kind().is_thumb()),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .iter()
        .filter_map(|x| Some((x.episode_id?, Image::new(x, &url_signer, now))))
        .collect();
    Ok(Output::Episodes(
        episodes
            .into_iter()
            .map(|x| {
                let media_file = x.media_file_id.and_then(|id| media_files.get(&id));
                let state = states
                    .get(&x.id)
                    .map(|x| x.watch_state())
                    .unwrap_or_default();
                Episode {
                    runtime_minutes: media_file.and_then(MediaFile::runtime_minutes),
                    progress: state.progress(media_file.and_then(|x| x.duration_ms)),
                    watched: state.watched,
                    thumb: thumbs.get(&x.id).cloned(),
                    id: x.id,
                    number: x.number,
                    title: x.title,
                    plot: x.overview,
                }
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::add_media_file;
    use crate::test::add_season;
    use crate::test::episode;
    use crate::test::mock_db_context;
    use crate::test::show;
    use crate::test::stop_at;
    use crate::test::url_signer;
    use crate::test::watch_to_the_end;
    use domain::ArtworkKind;
    use jiff::SignedDuration;
    use jiff::Timestamp;
    use test_fixtures::FakeClock;

    fn dependencies(db_context: &DbContext) -> Dependencies {
        Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(FakeClock::new().reader()),
            url_signer: url_signer(),
        }
    }

    async fn list(db_context: &DbContext, season_id: Uuid) -> Output {
        call(dependencies(db_context), Uuid::new_v4(), season_id)
            .await
            .unwrap()
    }

    async fn add_season_of(db_context: &mut DbContext, number: i64) -> Uuid {
        let show = show(db_context, "Severance")
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        add_season(db_context, show.id, number).await
    }

    #[tokio::test]
    async fn a_season_lists_its_episodes_in_the_library_in_order_with_runtime_plot_and_thumb() {
        let mut db_context = mock_db_context().await;
        let season_id = add_season_of(&mut db_context, 1).await;
        let media_file = db_context
            .media_files()
            .create()
            .path("/media/TV/Severance/Season 01/Severance - S01E02-E03.mkv")
            .duration_ms(Some(2_829_999))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        for (number, title, in_library, media_file_id) in [
            (3, "In Perpetuity", true, Some(media_file.id)),
            (4, "The You You Are", false, None),
            (1, "Good News About Hell", true, None),
            (2, "Half Loop", true, Some(media_file.id)),
        ] {
            episode(&db_context, season_id)
                .number(number)
                .title(title)
                .overview(Some(format!("The plot of {title}.")))
                .in_library(in_library)
                .media_file_id(media_file_id)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        let half_loop = db_context
            .episodes()
            .filter(domain::Episode::fields().title().eq("Half Loop"))
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap();
        let thumb = db_context
            .artwork()
            .create()
            .episode_id(Some(half_loop.id))
            .kind(ArtworkKind::Thumb)
            .path("/media/TV/Severance/Season 01/Severance - S01E02-E03-thumb.jpg")
            .modified_at(Timestamp::UNIX_EPOCH)
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        let Output::Episodes(episodes) = list(&db_context, season_id).await else {
            panic!("the season is not found");
        };

        assert_eq!(
            episodes
                .iter()
                .map(|x| (
                    x.number,
                    x.title.as_str(),
                    x.runtime_minutes,
                    x.plot.clone().unwrap()
                ))
                .collect::<Vec<_>>(),
            [
                (
                    1,
                    "Good News About Hell",
                    None,
                    "The plot of Good News About Hell.".to_owned()
                ),
                (
                    2,
                    "Half Loop",
                    Some(47),
                    "The plot of Half Loop.".to_owned()
                ),
                (
                    3,
                    "In Perpetuity",
                    Some(47),
                    "The plot of In Perpetuity.".to_owned()
                ),
            ]
        );
        assert_eq!(episodes[1].id, half_loop.id);
        assert_eq!(
            episodes[1]
                .thumb
                .as_ref()
                .map(|x| (x.id, x.urls.iter().map(|x| x.width).collect::<Vec<_>>())),
            Some((thumb.id, vec![480, 960]))
        );
        assert!(episodes[0].thumb.is_none());
    }

    #[tokio::test]
    async fn an_unknown_season_or_one_without_episodes_in_the_library_is_not_found() {
        let mut db_context = mock_db_context().await;
        let out_of_library = add_season_of(&mut db_context, 3).await;
        episode(&db_context, out_of_library)
            .in_library(false)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let empty = add_season_of(&mut db_context, 4).await;

        for season_id in [out_of_library, empty, Uuid::new_v4()] {
            let output = list(&db_context, season_id).await;

            assert!(matches!(output, Output::NotFound), "{output:?}");
        }
    }

    #[tokio::test]
    async fn each_episode_carries_the_users_progress_and_watched_flag() {
        let mut db_context = mock_db_context().await;
        let season_id = add_season_of(&mut db_context, 1).await;
        let mut files = Vec::new();
        for number in 1..=3 {
            let media_file = add_media_file(&db_context, Some(2_400_000)).await;
            episode(&db_context, season_id)
                .number(number)
                .media_file_id(Some(media_file))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            files.push(media_file);
        }
        let (noor, ravi) = (Uuid::new_v4(), Uuid::new_v4());
        watch_to_the_end(&db_context, noor, files[0], "2026-11-01T20:00:00Z").await;
        stop_at(
            &db_context,
            noor,
            files[1],
            SignedDuration::from_mins(10),
            "2026-11-02T20:00:00Z",
        )
        .await;

        let mut views = Vec::new();
        for user_id in [noor, ravi] {
            let Output::Episodes(episodes) = call(dependencies(&db_context), user_id, season_id)
                .await
                .unwrap()
            else {
                panic!("the season is not found");
            };
            views.push(
                episodes
                    .iter()
                    .map(|x| (x.progress, x.watched))
                    .collect::<Vec<_>>(),
            );
        }

        assert_eq!(
            views,
            [
                [(None, true), (Some(0.25), false), (None, false)],
                [(None, false), (None, false), (None, false)],
            ]
        );
    }
}
