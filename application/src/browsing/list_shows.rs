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
use crate::ports::persistence::DbScope;
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
    let mut db_context = dependencies.into().db_context;
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
    Ok(Output {
        tiles: shows
            .into_iter()
            .map(|show| Tile {
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
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;

    async fn add_show(
        db_context: &mut DbContext,
        title: &str,
        sort_title: Option<&str>,
        seasons: &[(i64, bool)],
    ) {
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
    }

    async fn list(db_context: &DbContext, after: Option<Cursor>) -> Output {
        call(
            Dependencies {
                db_context: db_context.clone(),
            },
            after,
        )
        .await
        .unwrap()
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
}
