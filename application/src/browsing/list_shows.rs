mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::browsing::Cursor;
use crate::browsing::SortKey;
use crate::browsing::Tile;
use crate::browsing::page;
use crate::browsing::season_counts;
use crate::browsing::title_images;
use crate::ports::persistence::DbScope;
use domain::Artwork;
use domain::Episode;
use domain::GridSort;
use domain::Season;
use domain::Title;
use std::collections::HashMap;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    genre: Option<String>,
    sort: GridSort,
    after: Option<Cursor>,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        url_signer,
    } = dependencies.into();
    let shows: Vec<_> = db_context
        .shows()
        .filter(domain::Show::in_library_filter())
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
                .and(Season::in_library_filter()),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let season_counts = season_counts(&seasons);
    let mut show_episodes = HashMap::<Uuid, Vec<Episode>>::new();
    if sort != GridSort::Title {
        let show_of_season: HashMap<_, _> = seasons.iter().map(|x| (x.id, x.show_id)).collect();
        for episode in db_context
            .episodes()
            .filter(Episode::in_library_filter())
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
        {
            if let Some(show_id) = show_of_season.get(&episode.season_id) {
                show_episodes.entry(*show_id).or_default().push(episode);
            }
        }
    }
    let shows = shows
        .into_iter()
        .map(|show| {
            let title = Title::show(&show, show_episodes.get(&show.id).into_iter().flatten());
            let key = SortKey::new(sort, &title);
            (Cursor { key, id: show.id }, show)
        })
        .collect();
    let (shows, next) = page(shows, after.as_ref());
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
            .map(|show| {
                let images = images.remove(&show.id).unwrap_or_default();
                let season_count = season_counts.get(&show.id).copied().unwrap_or_default();
                Tile::show(show, season_count, images)
            })
            .collect(),
        next,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browsing::TileKind;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::add_season;
    use crate::test::episode;
    use crate::test::mock_db_context;
    use crate::test::show;
    use crate::test::url_signer;
    use domain::ArtworkKind;
    use domain::Show;
    use jiff::Timestamp;
    use jiff::civil::Date;
    use jiff::civil::date;
    use test_fixtures::FakeClock;
    use toasty::schema::Model;
    use uuid::Uuid;

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

    struct Episode {
        season: i64,
        in_library: bool,
        added_at: Option<&'static str>,
        air_date: Option<Date>,
    }

    const IN_LIBRARY: Episode = Episode {
        season: 1,
        in_library: true,
        added_at: None,
        air_date: None,
    };

    async fn add_show(
        db_context: &mut DbContext,
        show: <Show as Model>::Create,
        episodes: &[Episode],
    ) -> Uuid {
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
                .in_library(x.in_library)
                .added_at(x.added_at.map(|x| x.parse::<Timestamp>().unwrap()))
                .air_date(x.air_date)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        show.id
    }

    async fn list(
        db_context: &DbContext,
        genre: Option<&str>,
        sort: GridSort,
        after: Option<Cursor>,
    ) -> Output {
        call(
            dependencies(db_context),
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
    async fn the_page_lists_every_show_with_an_episode_in_the_library_by_sort_title() {
        let mut db_context = mock_db_context().await;
        for (title, sort_title, in_library) in [
            ("Severance", None, true),
            ("The Expanse", Some("Expanse"), true),
            ("Gone", None, false),
            ("andor", None, true),
        ] {
            let show = show(&db_context, title).sort_title(sort_title.map(str::to_owned));
            add_show(
                &mut db_context,
                show,
                &[Episode {
                    in_library,
                    ..IN_LIBRARY
                }],
            )
            .await;
        }

        let output = list(&db_context, None, GridSort::Title, None).await;

        assert_eq!(titles(&output), ["andor", "The Expanse", "Severance"]);
        assert_eq!(output.next, None);
    }

    #[tokio::test]
    async fn a_show_counts_its_numbered_seasons_with_an_episode_in_the_library() {
        let mut db_context = mock_db_context().await;
        let show = show(&db_context, "Severance");
        add_show(
            &mut db_context,
            show,
            &[
                Episode {
                    season: 0,
                    ..IN_LIBRARY
                },
                Episode {
                    season: 1,
                    ..IN_LIBRARY
                },
                Episode {
                    season: 2,
                    ..IN_LIBRARY
                },
                Episode {
                    season: 3,
                    in_library: false,
                    ..IN_LIBRARY
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
            add_show(&mut db_context, show, &[IN_LIBRARY]).await;
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
            add_show(&mut db_context, show, &[IN_LIBRARY]).await;
        }

        let output = list(&db_context, Some("Drama"), GridSort::Title, None).await;

        assert_eq!(titles(&output), ["Andor", "The Bear"]);
    }

    #[tokio::test]
    async fn the_added_sort_lists_the_show_with_the_newest_episode_in_the_library_first() {
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
                    ..IN_LIBRARY
                })
                .chain([Episode {
                    in_library: false,
                    added_at: Some("2026-10-30T08:00:00Z"),
                    ..IN_LIBRARY
                }])
                .collect();
            add_show(&mut db_context, show, &episodes).await;
        }

        let output = list(&db_context, None, GridSort::Added, None).await;

        assert_eq!(titles(&output), ["Severance", "Andor", "The Bear"]);
    }

    #[tokio::test]
    async fn the_released_sort_lists_the_show_with_the_newest_air_date_in_the_library_first() {
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
                    ..IN_LIBRARY
                })
                .chain([Episode {
                    in_library: false,
                    air_date: Some(date(2026, 1, 1)),
                    ..IN_LIBRARY
                }])
                .collect();
            add_show(&mut db_context, show, &episodes).await;
        }

        let output = list(&db_context, None, GridSort::Released, None).await;

        assert_eq!(titles(&output), ["Andor", "Severance", "The Bear"]);
    }

    #[tokio::test]
    async fn a_show_tile_carries_the_show_fanart_and_logo_but_no_episode_thumb() {
        let mut db_context = mock_db_context().await;
        let show = show(&db_context, "Severance");
        let show_id = add_show(&mut db_context, show, &[IN_LIBRARY]).await;
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

        let output = list(&db_context, None, GridSort::Title, None).await;

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
