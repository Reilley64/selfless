mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::browsing::Cursor;
use crate::browsing::SortKey;
use crate::browsing::Tile;
use crate::browsing::TileKind;
use crate::browsing::page;
use crate::browsing::visible_season;
use crate::browsing::visible_show;
use crate::ports::persistence::DbScope;
use domain::GridSort;
use domain::Season;
use domain::sort_key;
use std::cmp::Reverse;
use std::collections::HashMap;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    genre: Option<String>,
    sort: GridSort,
    after: Option<Cursor>,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let shows: Vec<_> = db_context
        .shows()
        .filter(visible_show())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .filter(|show| genre.as_ref().is_none_or(|x| show.genres.contains(x)))
        .collect();
    let seasons = db_context
        .seasons()
        .filter(
            Season::fields()
                .show_id()
                .in_list(shows.iter().map(|x| x.id).collect::<Vec<_>>())
                .and(visible_season()),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let mut season_counts = HashMap::<Uuid, i64>::new();
    for season in seasons.iter().filter(|x| x.number > 0) {
        *season_counts.entry(season.show_id).or_default() += 1;
    }
    let mut newest = HashMap::new();
    if sort != GridSort::Title {
        let show_of_season: HashMap<_, _> = seasons.iter().map(|x| (x.id, x.show_id)).collect();
        for episode in db_context
            .episodes()
            .filter(domain::Episode::fields().visible().eq(true))
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
        {
            if let Some(show_id) = show_of_season.get(&episode.season_id) {
                let (added_at, air_date) = newest.entry(*show_id).or_insert((None, None));
                *added_at = episode.added_at.max(*added_at);
                *air_date = episode.air_date.max(*air_date);
            }
        }
    }
    let shows = shows
        .into_iter()
        .map(|show| {
            let (added_at, air_date) = newest.get(&show.id).copied().unwrap_or_default();
            let key = match sort {
                GridSort::Title => {
                    SortKey::Title(sort_key(show.sort_title.as_deref(), &show.title).0)
                }
                GridSort::Added => SortKey::Added(Reverse(added_at)),
                GridSort::Released => SortKey::Released(Reverse(air_date)),
            };
            (Cursor { key, id: show.id }, show)
        })
        .collect();
    let (shows, next) = page(shows, after.as_ref());
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
    use crate::test::add_season;
    use crate::test::episode;
    use crate::test::mock_db_context;
    use crate::test::show;
    use domain::Show;
    use jiff::Timestamp;
    use jiff::civil::Date;
    use jiff::civil::date;
    use toasty::schema::Model;

    struct Episode {
        season: i64,
        visible: bool,
        added_at: Option<&'static str>,
        air_date: Option<Date>,
    }

    const VISIBLE: Episode = Episode {
        season: 1,
        visible: true,
        added_at: None,
        air_date: None,
    };

    async fn add_show(
        db_context: &mut DbContext,
        show: <Show as Model>::Create,
        episodes: &[Episode],
    ) {
        let show = show.exec(db_context.executor_mut()).await.unwrap();
        let mut seasons = HashMap::new();
        for x in episodes {
            let season_id = match seasons.get(&x.season) {
                Some(id) => *id,
                None => {
                    let id = add_season(db_context, show.id, x.season).await;
                    seasons.insert(x.season, id);
                    id
                }
            };
            episode(db_context, season_id)
                .visible(x.visible)
                .added_at(x.added_at.map(|x| x.parse::<Timestamp>().unwrap()))
                .air_date(x.air_date)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
    }

    async fn list(
        db_context: &DbContext,
        genre: Option<&str>,
        sort: GridSort,
        after: Option<Cursor>,
    ) -> Output {
        call(
            Dependencies {
                db_context: db_context.clone(),
            },
            genre.map(str::to_owned),
            sort,
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
        for (title, sort_title, visible) in [
            ("Severance", None, true),
            ("The Expanse", Some("Expanse"), true),
            ("Hidden", None, false),
            ("andor", None, true),
        ] {
            let show = show(&db_context, title).sort_title(sort_title.map(str::to_owned));
            add_show(&mut db_context, show, &[Episode { visible, ..VISIBLE }]).await;
        }

        let output = list(&db_context, None, GridSort::Title, None).await;

        assert_eq!(titles(&output), ["andor", "The Expanse", "Severance"]);
        assert_eq!(output.next, None);
    }

    #[tokio::test]
    async fn a_show_counts_its_numbered_seasons_with_a_visible_episode() {
        let mut db_context = mock_db_context().await;
        let show = show(&db_context, "Severance");
        add_show(
            &mut db_context,
            show,
            &[
                Episode {
                    season: 0,
                    ..VISIBLE
                },
                Episode {
                    season: 1,
                    ..VISIBLE
                },
                Episode {
                    season: 2,
                    ..VISIBLE
                },
                Episode {
                    season: 3,
                    visible: false,
                    ..VISIBLE
                },
            ],
        )
        .await;

        let output = list(&db_context, None, GridSort::Title, None).await;

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
            let show = show(&db_context, &format!("Show {number:02}"));
            add_show(&mut db_context, show, &[VISIBLE]).await;
        }

        let first = list(&db_context, None, GridSort::Title, None).await;
        let second = list(&db_context, None, GridSort::Title, first.next.clone()).await;

        assert_eq!(first.tiles.len(), 60);
        assert_eq!(titles(&second), ["Show 60"]);
        assert_eq!(second.next, None);
    }

    #[tokio::test]
    async fn the_genre_filter_keeps_only_the_shows_of_that_genre() {
        let mut db_context = mock_db_context().await;
        for (title, genres) in [
            ("Andor", &["Drama", "Science Fiction"][..]),
            ("Severance", &["Mystery"][..]),
            ("The Bear", &["Comedy", "Drama"][..]),
        ] {
            let show = show(&db_context, title)
                .genres(genres.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>());
            add_show(&mut db_context, show, &[VISIBLE]).await;
        }

        let output = list(&db_context, Some("Drama"), GridSort::Title, None).await;

        assert_eq!(titles(&output), ["Andor", "The Bear"]);
    }

    #[tokio::test]
    async fn the_added_sort_lists_the_show_with_the_newest_visible_episode_first() {
        let mut db_context = mock_db_context().await;
        for (title, episodes) in [
            (
                "Andor",
                [
                    Some("2026-10-20T08:00:00Z"),
                    Some("2026-10-21T08:00:00Z"),
                    None,
                ],
            ),
            ("Severance", [Some("2026-10-22T08:00:00Z"), None, None]),
            ("The Bear", [None, None, None]),
        ] {
            let show = show(&db_context, title);
            let episodes: Vec<_> = episodes
                .into_iter()
                .map(|added_at| Episode {
                    added_at,
                    ..VISIBLE
                })
                .chain([Episode {
                    visible: false,
                    added_at: Some("2026-10-30T08:00:00Z"),
                    ..VISIBLE
                }])
                .collect();
            add_show(&mut db_context, show, &episodes).await;
        }

        let output = list(&db_context, None, GridSort::Added, None).await;

        assert_eq!(titles(&output), ["Severance", "Andor", "The Bear"]);
    }

    #[tokio::test]
    async fn the_released_sort_lists_the_show_with_the_newest_visible_air_date_first() {
        let mut db_context = mock_db_context().await;
        for (title, air_dates) in [
            ("Andor", [Some(date(2022, 9, 21)), Some(date(2025, 4, 22))]),
            ("Severance", [Some(date(2025, 1, 17)), None]),
            ("The Bear", [None, None]),
        ] {
            let show = show(&db_context, title);
            let episodes: Vec<_> = air_dates
                .into_iter()
                .map(|air_date| Episode {
                    air_date,
                    ..VISIBLE
                })
                .chain([Episode {
                    visible: false,
                    air_date: Some(date(2026, 1, 1)),
                    ..VISIBLE
                }])
                .collect();
            add_show(&mut db_context, show, &episodes).await;
        }

        let output = list(&db_context, None, GridSort::Released, None).await;

        assert_eq!(titles(&output), ["Andor", "Severance", "The Bear"]);
    }
}
