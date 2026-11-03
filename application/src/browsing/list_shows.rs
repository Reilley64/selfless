mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::browsing::Cursor;
use crate::browsing::Tile;
use crate::browsing::TileKind;
use crate::browsing::page;
use crate::browsing::title_images;
use crate::ports::persistence::DbScope;
use domain::Artwork;
use domain::Episode;
use domain::Season;
use domain::Show;
use domain::sort_key;
use std::collections::HashMap;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    after: Option<Cursor>,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        url_signer,
    } = dependencies.into();
    let has_visible_episode = || {
        Season::fields()
            .episodes()
            .any(Episode::fields().visible().eq(true))
    };
    let shows = Show::filter(Show::fields().seasons().any(has_visible_episode()))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|show| {
            let cursor = Cursor {
                sort_title: sort_key(show.sort_title.as_deref(), &show.title).0,
                id: show.id,
            };
            (cursor, show)
        })
        .collect();
    let (shows, next) = page(shows, after.as_ref());
    let mut season_counts = HashMap::<Uuid, i64>::new();
    for season in Season::filter(
        Season::fields()
            .show_id()
            .in_list(shows.iter().map(|x| x.id).collect::<Vec<_>>())
            .and(Season::fields().number().gt(0))
            .and(has_visible_episode()),
    )
    .exec(db_context.executor_mut())
    .await
    .map_err(Error::caused_by)?
    {
        *season_counts.entry(season.show_id).or_default() += 1;
    }
    let mut images = title_images(
        &mut db_context,
        |x| x.show_id,
        Artwork::fields()
            .show_id()
            .in_list(shows.iter().map(|x| x.id).collect::<Vec<_>>()),
        &url_signer,
        clock.call(),
    )
    .await
    .map_err(Error::caused_by)?;
    Ok(Output {
        tiles: shows
            .into_iter()
            .map(|show| Tile {
                images: images.remove(&show.id).unwrap_or_default(),
                id: show.id,
                title: show.title,
                year: show.year,
                certification: show.certification,
                genres: show.genres,
                plot: show.overview,
                kind: TileKind::Show {
                    season_count: season_counts.get(&show.id).copied().unwrap_or_default(),
                },
            })
            .collect(),
        next,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;
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

    async fn add_artwork(
        db_context: &mut DbContext,
        show_id: Uuid,
        episode_id: Option<Uuid>,
        kind: ArtworkKind,
    ) -> Uuid {
        db_context
            .artwork()
            .create()
            .show_id(Some(show_id))
            .episode_id(episode_id)
            .kind(kind)
            .path(format!("/mnt/media/shows/Severance/{kind:?}"))
            .modified_at(Timestamp::UNIX_EPOCH)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    async fn add_show(
        db_context: &mut DbContext,
        title: &str,
        sort_title: Option<&str>,
        seasons: &[(i64, bool)],
    ) -> Uuid {
        let tvdb_id = Uuid::new_v4().as_u64_pair().0 as i64;
        let show = Show::create()
            .tvdb_id(tvdb_id)
            .sonarr_id(tvdb_id)
            .title(title)
            .year(Some(2022))
            .genres(Vec::<String>::new())
            .sort_title(sort_title.map(str::to_owned))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        for (number, visible) in seasons {
            let season = Season::create()
                .show_id(show.id)
                .number(*number)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            Episode::create()
                .season_id(season.id)
                .tvdb_id(Uuid::new_v4().as_u64_pair().0 as i64)
                .number(1)
                .title("Good News About Hell")
                .visible(*visible)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        show.id
    }

    async fn list(db_context: &DbContext, after: Option<Cursor>) -> Output {
        call(dependencies(db_context), after).await.unwrap()
    }

    fn titles(output: &Output) -> Vec<&str> {
        output.tiles.iter().map(|x| x.title.as_str()).collect()
    }

    #[tokio::test]
    async fn the_page_lists_every_show_with_a_visible_episode_by_sort_title() {
        let mut db_context = mock_db_context().await;
        add_show(&mut db_context, "Severance", None, &[(1, true)]).await;
        add_show(
            &mut db_context,
            "The Expanse",
            Some("Expanse"),
            &[(1, true)],
        )
        .await;
        add_show(&mut db_context, "Hidden", None, &[(1, false)]).await;
        add_show(&mut db_context, "andor", None, &[(1, true)]).await;

        let output = list(&db_context, None).await;

        assert_eq!(titles(&output), ["andor", "The Expanse", "Severance"]);
        assert_eq!(output.next, None);
    }

    #[tokio::test]
    async fn a_show_counts_its_numbered_seasons_with_a_visible_episode() {
        let mut db_context = mock_db_context().await;
        add_show(
            &mut db_context,
            "Severance",
            None,
            &[(0, true), (1, true), (2, true), (3, false)],
        )
        .await;

        let output = list(&db_context, None).await;

        assert!(
            matches!(output.tiles[0].kind, TileKind::Show { season_count: 2 }),
            "{:?}",
            output.tiles[0].kind
        );
    }

    #[tokio::test]
    async fn the_next_page_starts_after_its_cursor() {
        let mut db_context = mock_db_context().await;
        for number in 0..61 {
            add_show(
                &mut db_context,
                &format!("Show {number:02}"),
                None,
                &[(1, true)],
            )
            .await;
        }

        let first = list(&db_context, None).await;
        let second = list(&db_context, first.next.clone()).await;

        assert_eq!(first.tiles.len(), 60);
        assert_eq!(titles(&second), ["Show 60"]);
        assert_eq!(second.next, None);
    }

    #[tokio::test]
    async fn a_show_tile_carries_the_show_fanart_and_logo_but_no_episode_thumb() {
        let mut db_context = mock_db_context().await;
        let show_id = add_show(&mut db_context, "Severance", None, &[(1, true)]).await;
        let episode_id = db_context
            .episodes()
            .all()
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap()
            .id;
        let fanart = add_artwork(&mut db_context, show_id, None, ArtworkKind::Fanart).await;
        add_artwork(
            &mut db_context,
            show_id,
            Some(episode_id),
            ArtworkKind::Thumb,
        )
        .await;

        let output = list(&db_context, None).await;

        let images = &output.tiles[0].images;
        assert_eq!(images.fanart.as_ref().map(|x| x.id), Some(fanart));
        assert_eq!(
            images
                .fanart
                .as_ref()
                .unwrap()
                .urls
                .iter()
                .map(|x| x.width)
                .collect::<Vec<_>>(),
            [480, 960, 1280, 1920]
        );
        assert!(images.logo.is_none());
        assert!(images.thumb.is_none());
    }
}
