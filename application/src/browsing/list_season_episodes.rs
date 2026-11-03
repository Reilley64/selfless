mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Episode;
pub use output::Output;

use crate::browsing::Image;
use crate::ports::persistence::DbScope;
use domain::Artwork;
use domain::MediaFile;
use std::collections::HashMap;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, season_id: Uuid) -> Result<Output, Error> {
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
    let runtimes: HashMap<Uuid, Option<i64>> = db_context
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
        .iter()
        .map(|x| (x.id, x.runtime_minutes()))
        .collect();
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
            .map(|x| Episode {
                runtime_minutes: x
                    .media_file_id
                    .and_then(|id| runtimes.get(&id).copied().flatten()),
                thumb: thumbs.get(&x.id).cloned(),
                media_file_id: x.media_file_id,
                id: x.id,
                number: x.number,
                title: x.title,
                plot: x.overview,
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::add_season;
    use crate::test::episode;
    use crate::test::mock_db_context;
    use crate::test::show;
    use crate::test::url_signer;
    use domain::ArtworkKind;
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
        call(dependencies(db_context), season_id).await.unwrap()
    }

    async fn add_season_of(db_context: &mut DbContext, number: i64) -> Uuid {
        let show = show(db_context, "Severance")
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        add_season(db_context, show.id, number).await
    }

    #[tokio::test]
    async fn a_season_lists_its_visible_episodes_in_order_with_runtime_plot_and_thumb() {
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
        for (number, title, visible, media_file_id) in [
            (3, "In Perpetuity", true, Some(media_file.id)),
            (4, "The You You Are", false, None),
            (1, "Good News About Hell", true, None),
            (2, "Half Loop", true, Some(media_file.id)),
        ] {
            episode(&db_context, season_id)
                .number(number)
                .title(title)
                .overview(Some(format!("The plot of {title}.")))
                .in_library(visible)
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
    async fn an_unknown_season_or_one_without_visible_episodes_is_not_found() {
        let mut db_context = mock_db_context().await;
        let hidden = add_season_of(&mut db_context, 3).await;
        episode(&db_context, hidden)
            .in_library(false)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let empty = add_season_of(&mut db_context, 4).await;

        for season_id in [hidden, empty, Uuid::new_v4()] {
            let output = list(&db_context, season_id).await;

            assert!(matches!(output, Output::NotFound), "{output:?}");
        }
    }
}
