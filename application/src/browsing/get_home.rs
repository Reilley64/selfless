mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::FeaturedTitle;
pub use output::Output;
pub use output::Row;
pub use output::RowKind;

use crate::browsing::Tile;
use crate::browsing::season_counts;
use crate::browsing::title_images;
use crate::ports::persistence::DbScope;
use domain::Artwork;
use domain::Episode;
use domain::Movie;
use domain::Season;
use domain::Show;
use domain::first_episode;
use domain::sort_key;
use jiff::Timestamp;
use jiff::civil::Date;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::collections::HashSet;
use uuid::Uuid;

const ROW_SIZE: usize = 20;
const GENRE_ROWS: usize = 6;

struct Title<'a> {
    id: Uuid,
    genres: &'a [String],
    sort_key: (String, String),
    added_at: Option<Timestamp>,
    released: Option<Date>,
}

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        url_signer,
    } = dependencies.into();
    let movies = db_context
        .movies()
        .filter(Movie::in_library_filter())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let shows = db_context
        .shows()
        .filter(Show::in_library_filter())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let seasons = db_context
        .seasons()
        .filter(Season::in_library_filter())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let episodes = db_context
        .episodes()
        .filter(Episode::in_library_filter())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;

    let season_of: HashMap<_, _> = seasons.iter().map(|x| (x.id, x)).collect();
    let mut newest = HashMap::<Uuid, (Option<Timestamp>, Option<Date>)>::new();
    for episode in &episodes {
        if let Some(season) = season_of.get(&episode.season_id) {
            let (added_at, air_date) = newest.entry(season.show_id).or_default();
            *added_at = episode.added_at.max(*added_at);
            *air_date = episode.air_date.max(*air_date);
        }
    }
    let titles: Vec<Title> = movies
        .iter()
        .map(|movie| Title {
            id: movie.id,
            genres: &movie.genres,
            sort_key: sort_key(movie.sort_title.as_deref(), &movie.title),
            added_at: movie.added_at,
            released: movie.premiered,
        })
        .chain(shows.iter().map(|show| {
            let (added_at, released) = newest.get(&show.id).copied().unwrap_or_default();
            Title {
                id: show.id,
                genres: &show.genres,
                sort_key: sort_key(show.sort_title.as_deref(), &show.title),
                added_at,
                released,
            }
        }))
        .collect();

    let mut recently_added: Vec<&Title> = titles.iter().filter(|x| x.added_at.is_some()).collect();
    recently_added.sort_by(|a, b| {
        (Reverse(a.added_at), &a.sort_key, a.id).cmp(&(Reverse(b.added_at), &b.sort_key, b.id))
    });
    recently_added.truncate(ROW_SIZE);
    let mut genre_counts = HashMap::<&str, usize>::new();
    for name in titles.iter().flat_map(|x| x.genres) {
        *genre_counts.entry(name).or_default() += 1;
    }
    let mut top_genres: Vec<_> = genre_counts.into_iter().collect();
    top_genres.sort_by(|a, b| (Reverse(a.1), a.0).cmp(&(Reverse(b.1), b.0)));
    top_genres.truncate(GENRE_ROWS);
    let mut by_sort_title: Vec<&Title> = titles.iter().collect();
    by_sort_title.sort_by(|a, b| (&a.sort_key, a.id).cmp(&(&b.sort_key, b.id)));
    let genre_rows = top_genres.into_iter().map(|(name, _)| {
        (
            RowKind::Genre(name.to_owned()),
            by_sort_title
                .iter()
                .filter(|x| x.genres.iter().any(|x| x == name))
                .take(ROW_SIZE)
                .map(|x| x.id)
                .collect::<Vec<_>>(),
        )
    });
    let rows: Vec<(RowKind, Vec<Uuid>)> = [(
        RowKind::RecentlyAdded,
        recently_added.iter().map(|x| x.id).collect::<Vec<_>>(),
    )]
    .into_iter()
    .chain(genre_rows)
    .filter(|(_, ids)| !ids.is_empty())
    .collect();

    let featured = titles
        .iter()
        .filter(|x| x.released.is_some())
        .min_by(|a, b| {
            (Reverse(a.released), &a.sort_key, a.id).cmp(&(Reverse(b.released), &b.sort_key, b.id))
        })
        .map(|x| x.id);
    let shown: HashSet<Uuid> = rows
        .iter()
        .flat_map(|(_, ids)| ids.iter().copied())
        .chain(featured)
        .collect();
    let play_episode_id = featured.and_then(|id| {
        first_episode(
            episodes
                .iter()
                .filter_map(|x| Some((*season_of.get(&x.season_id)?, x)))
                .filter(|(season, _)| season.show_id == id),
        )
        .map(|(_, episode)| episode.id)
    });
    let now = clock.call();
    let mut movie_images = title_images(
        &mut db_context,
        |x| x.movie_id,
        Artwork::fields().movie_id().in_list(
            movies
                .iter()
                .map(|x| x.id)
                .filter(|x| shown.contains(x))
                .collect::<Vec<_>>(),
        ),
        &url_signer,
        now,
    )
    .await
    .map_err(Error::caused_by)?;
    let mut show_images = title_images(
        &mut db_context,
        |x| x.show_id,
        Artwork::fields().show_id().in_list(
            shows
                .iter()
                .map(|x| x.id)
                .filter(|x| shown.contains(x))
                .collect::<Vec<_>>(),
        ),
        &url_signer,
        now,
    )
    .await
    .map_err(Error::caused_by)?;
    let season_counts = season_counts(&seasons);
    let tiles: HashMap<Uuid, Tile> = movies
        .into_iter()
        .filter(|x| shown.contains(&x.id))
        .map(|movie| {
            let images = movie_images.remove(&movie.id).unwrap_or_default();
            (movie.id, Tile::movie(movie, images))
        })
        .chain(
            shows
                .into_iter()
                .filter(|x| shown.contains(&x.id))
                .map(|show| {
                    let images = show_images.remove(&show.id).unwrap_or_default();
                    let season_count = season_counts.get(&show.id).copied().unwrap_or_default();
                    (show.id, Tile::show(show, season_count, images))
                }),
        )
        .collect();

    Ok(Output {
        featured: featured.map(|id| FeaturedTitle {
            tile: tiles[&id].clone(),
            play_episode_id,
        }),
        rows: rows
            .into_iter()
            .map(|(kind, ids)| Row {
                kind,
                tiles: ids.iter().map(|x| tiles[x].clone()).collect(),
            })
            .collect(),
    })
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
    use crate::test::url_signer;
    use domain::Show;
    use jiff::civil::Date;
    use jiff::civil::date;
    use std::collections::HashMap;
    use test_fixtures::FakeClock;
    use toasty::schema::Model;
    use uuid::Uuid;

    struct Episode {
        season: i64,
        number: i64,
        in_library: bool,
        added_at: Option<&'static str>,
        air_date: Option<Date>,
    }

    const EPISODE: Episode = Episode {
        season: 1,
        number: 1,
        in_library: true,
        added_at: None,
        air_date: None,
    };

    async fn add_show(
        db_context: &mut DbContext,
        show: <Show as Model>::Create,
        episodes: &[Episode],
    ) -> HashMap<(i64, i64), Uuid> {
        let show = show.exec(db_context.executor_mut()).await.unwrap();
        let mut seasons = HashMap::new();
        let mut ids = HashMap::new();
        for x in episodes {
            let season_id = match seasons.get(&x.season) {
                Some(id) => *id,
                None => {
                    let id = add_season(db_context, show.id, x.season).await;
                    seasons.insert(x.season, id);
                    id
                }
            };
            let episode = episode(db_context, season_id)
                .number(x.number)
                .in_library(x.in_library)
                .added_at(x.added_at.map(at))
                .air_date(x.air_date)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            ids.insert((x.season, x.number), episode.id);
        }
        ids
    }

    async fn add_movie(db_context: &mut DbContext, title: &str, added_at: Option<&str>) {
        movie(db_context, title)
            .added_at(added_at.map(at))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }

    async fn home(db_context: &DbContext) -> Output {
        call(Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(FakeClock::new().reader()),
            url_signer: url_signer(),
        })
        .await
        .unwrap()
    }

    fn rows(output: &Output) -> Vec<(&RowKind, Vec<&str>)> {
        output
            .rows
            .iter()
            .map(|row| {
                (
                    &row.kind,
                    row.tiles.iter().map(|x| x.title.as_str()).collect(),
                )
            })
            .collect()
    }

    #[tokio::test]
    async fn recently_added_has_one_tile_per_title_and_a_show_sorts_by_its_newest_visible_episode()
    {
        let mut db_context = mock_db_context().await;
        add_movie(&mut db_context, "Arrival", Some("2026-10-29T08:00:00Z")).await;
        add_movie(&mut db_context, "Dune", Some("2026-10-20T08:00:00Z")).await;
        let severance = show(&db_context, "Severance");
        let season_pack: Vec<_> = (1..=3)
            .map(|number| Episode {
                number,
                added_at: Some("2026-10-25T08:00:00Z"),
                ..EPISODE
            })
            .chain([Episode {
                number: 4,
                in_library: false,
                added_at: Some("2026-10-30T08:00:00Z"),
                ..EPISODE
            }])
            .collect();
        add_show(&mut db_context, severance, &season_pack).await;
        let andor = show(&db_context, "Andor");
        add_show(
            &mut db_context,
            andor,
            &[
                Episode {
                    added_at: Some("2026-10-21T08:00:00Z"),
                    ..EPISODE
                },
                Episode {
                    number: 2,
                    added_at: Some("2026-10-27T08:00:00Z"),
                    ..EPISODE
                },
            ],
        )
        .await;

        let output = home(&db_context).await;

        assert_eq!(
            rows(&output),
            [(
                &RowKind::RecentlyAdded,
                vec!["Arrival", "Andor", "Severance", "Dune"]
            )]
        );
    }
    fn genres(names: &[&str]) -> Vec<String> {
        names.iter().map(|x| (*x).to_owned()).collect()
    }

    #[tokio::test]
    async fn the_genre_rows_are_the_6_genres_with_the_most_visible_titles_sorted_by_sort_title() {
        let mut db_context = mock_db_context().await;
        for (title, sort_title, names, visible) in [
            ("Arrival", None, &["Drama", "Mystery"][..], true),
            (
                "The Departed",
                Some("Departed"),
                &["Drama", "Crime", "Thriller"][..],
                true,
            ),
            ("Zodiac", None, &["Crime", "Mystery", "Thriller"][..], true),
            ("Airplane!", None, &["Comedy"][..], true),
            ("Heat", None, &["Action"][..], true),
            ("Hidden", None, &["Action"][..], false),
        ] {
            movie(&db_context, title)
                .sort_title(sort_title.map(str::to_owned))
                .genres(genres(names))
                .in_library(visible)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        for (title, names, visible) in [
            ("Severance", &["Drama", "Comedy", "Horror"][..], true),
            ("Barry", &["Drama", "Comedy", "Crime", "Horror"][..], true),
            ("Gone", &["Action", "Western"][..], false),
        ] {
            let show = show(&db_context, title).genres(genres(names));
            add_show(
                &mut db_context,
                show,
                &[Episode {
                    in_library: visible,
                    ..EPISODE
                }],
            )
            .await;
        }

        let output = home(&db_context).await;

        let genre = |name: &str| RowKind::Genre(name.to_owned());
        assert_eq!(
            rows(&output),
            [
                (
                    &genre("Drama"),
                    vec!["Arrival", "Barry", "The Departed", "Severance"]
                ),
                (&genre("Comedy"), vec!["Airplane!", "Barry", "Severance"]),
                (&genre("Crime"), vec!["Barry", "The Departed", "Zodiac"]),
                (&genre("Horror"), vec!["Barry", "Severance"]),
                (&genre("Mystery"), vec!["Arrival", "Zodiac"]),
                (&genre("Thriller"), vec!["The Departed", "Zodiac"]),
            ]
        );
    }

    #[tokio::test]
    async fn a_row_holds_up_to_20_tiles() {
        let mut db_context = mock_db_context().await;
        for number in 0..21 {
            movie(&db_context, &format!("Movie {number:02}"))
                .genres(genres(&["Drama"]))
                .added_at(Some(at("2026-10-01T08:00:00Z")))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        let output = home(&db_context).await;

        assert_eq!(
            output
                .rows
                .iter()
                .map(|x| x.tiles.len())
                .collect::<Vec<_>>(),
            [20, 20]
        );
    }
    #[tokio::test]
    async fn with_no_plays_the_featured_title_is_the_latest_released_and_ties_go_by_sort_title() {
        let mut db_context = mock_db_context().await;
        for (title, premiered) in [
            ("Sicario", Some(date(2025, 5, 13))),
            ("Arrival", Some(date(2016, 11, 11))),
            ("Dune", None),
        ] {
            movie(&db_context, title)
                .premiered(premiered)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        let andor = show(&db_context, "Andor");
        add_show(
            &mut db_context,
            andor,
            &[
                Episode {
                    air_date: Some(date(2022, 9, 21)),
                    ..EPISODE
                },
                Episode {
                    season: 2,
                    air_date: Some(date(2025, 5, 13)),
                    ..EPISODE
                },
            ],
        )
        .await;
        let severance = show(&db_context, "Severance");
        add_show(
            &mut db_context,
            severance,
            &[
                Episode {
                    air_date: Some(date(2025, 3, 21)),
                    ..EPISODE
                },
                Episode {
                    number: 2,
                    in_library: false,
                    air_date: Some(date(2026, 1, 1)),
                    ..EPISODE
                },
            ],
        )
        .await;

        let output = home(&db_context).await;

        assert_eq!(
            output.featured.map(|x| x.tile.title).as_deref(),
            Some("Andor")
        );
    }

    #[tokio::test]
    async fn with_no_dated_titles_there_is_no_featured_title() {
        let mut db_context = mock_db_context().await;
        movie(&db_context, "Dune")
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let severance = show(&db_context, "Severance");
        add_show(&mut db_context, severance, &[EPISODE]).await;

        let output = home(&db_context).await;

        assert!(output.featured.is_none(), "{:?}", output.featured);
    }

    #[tokio::test]
    async fn a_featured_show_plays_the_first_episode_of_its_first_numbered_season() {
        let mut db_context = mock_db_context().await;
        let severance = show(&db_context, "Severance");
        let dated = |season, number| Episode {
            season,
            number,
            air_date: Some(date(2022, 2, 18)),
            ..EPISODE
        };
        let episodes = add_show(
            &mut db_context,
            severance,
            &[dated(2, 1), dated(0, 1), dated(1, 2), dated(1, 1)],
        )
        .await;

        let output = home(&db_context).await;

        let featured = output.featured.unwrap();
        assert_eq!(featured.play_episode_id, Some(episodes[&(1, 1)]));
    }

    #[tokio::test]
    async fn a_featured_movie_plays_itself() {
        let mut db_context = mock_db_context().await;
        movie(&db_context, "Arrival")
            .premiered(Some(date(2016, 11, 11)))
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        let output = home(&db_context).await;

        let featured = output.featured.unwrap();
        assert_eq!(featured.tile.title, "Arrival");
        assert_eq!(featured.play_episode_id, None);
    }
}
