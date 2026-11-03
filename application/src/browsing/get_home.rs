mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::FeaturedTitle;
pub use output::Output;
pub use output::Row;

use crate::browsing::Image;
use crate::browsing::Tile;
use crate::browsing::movie_watches;
use crate::browsing::season_counts;
use crate::browsing::title_images;
use crate::ports::persistence::DbScope;
use domain::Artwork;
use domain::ContinueWatching;
use domain::Episode;
use domain::EpisodeWatchState;
use domain::MediaFile;
use domain::Movie;
use domain::MovieContinueWatchingRemoval;
use domain::MovieWatchState;
use domain::RowKind;
use domain::Season;
use domain::Show;
use domain::ShowContinueWatchingRemoval;
use domain::ShowEpisode;
use domain::Title;
use domain::continue_watching;
use domain::featured_title;
use domain::genre_rows;
use domain::popular_now;
use domain::popular_now_start;
use domain::recently_added;
use domain::show_play_target;
use std::collections::HashMap;
use std::collections::HashSet;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, user_id: Uuid) -> Result<Output, Error> {
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
        .filter(
            Season::fields()
                .show_id()
                .in_list(shows.iter().map(|x| x.id).collect::<Vec<_>>()),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let episodes = db_context
        .episodes()
        .filter(
            Episode::fields()
                .season_id()
                .in_list(seasons.iter().map(|x| x.id).collect::<Vec<_>>()),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;

    let now = clock.call();
    let user_movie_states = db_context
        .movie_watch_states()
        .filter(MovieWatchState::fields().user_id().eq(user_id))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let user_episode_states = db_context
        .episode_watch_states()
        .filter(EpisodeWatchState::fields().user_id().eq(user_id))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let movie_removals = db_context
        .movie_continue_watching_removals()
        .filter(MovieContinueWatchingRemoval::fields().user_id().eq(user_id))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let show_removals = db_context
        .show_continue_watching_removals()
        .filter(ShowContinueWatchingRemoval::fields().user_id().eq(user_id))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let recent_movie_states = db_context
        .movie_watch_states()
        .filter(
            MovieWatchState::fields()
                .last_played()
                .ge(popular_now_start(now)),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let recent_episode_states = db_context
        .episode_watch_states()
        .filter(
            EpisodeWatchState::fields()
                .last_played()
                .ge(popular_now_start(now)),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;

    let season_of: HashMap<_, _> = seasons.iter().map(|x| (x.id, x)).collect();
    let movie_of: HashMap<_, _> = movies.iter().map(|x| (x.id, x)).collect();
    let user_state_of: HashMap<_, _> = user_episode_states
        .iter()
        .map(|x| (x.episode_id, x))
        .collect();
    let mut show_episodes = HashMap::<Uuid, Vec<_>>::new();
    for episode in &episodes {
        if let Some(season) = season_of.get(&episode.season_id) {
            show_episodes
                .entry(season.show_id)
                .or_default()
                .push(ShowEpisode {
                    season,
                    episode,
                    watch_state: user_state_of.get(&episode.id).copied(),
                });
        }
    }
    let continue_watching = continue_watching(
        user_movie_states
            .iter()
            .filter_map(|x| Some((*movie_of.get(&x.movie_id)?, x))),
        show_episodes.values().cloned(),
        &movie_removals,
        &show_removals,
    );
    let episode_tiles: Vec<ShowEpisode> = continue_watching
        .iter()
        .filter_map(|x| match *x {
            ContinueWatching::Movie(..) => None,
            ContinueWatching::Resume(x) | ContinueWatching::NextUp(x) => Some(x),
        })
        .collect();
    let continue_watching: Vec<Uuid> = continue_watching
        .iter()
        .map(|x| match x {
            ContinueWatching::Movie(movie, _) => movie.id,
            ContinueWatching::Resume(x) | ContinueWatching::NextUp(x) => x.episode.id,
        })
        .collect();
    let show_of_episode: HashMap<Uuid, Uuid> = show_episodes
        .iter()
        .flat_map(|(show_id, episodes)| episodes.iter().map(|x| (x.episode.id, *show_id)))
        .collect();
    let popular_now = popular_now(
        recent_movie_states
            .iter()
            .filter(|x| movie_of.contains_key(&x.movie_id)),
        recent_episode_states
            .iter()
            .filter_map(|x| Some((*show_of_episode.get(&x.episode_id)?, x))),
        now,
    );
    let titles: Vec<Title> = movies
        .iter()
        .map(Title::movie)
        .chain(shows.iter().map(|show| {
            Title::show(
                show,
                show_episodes
                    .get(&show.id)
                    .into_iter()
                    .flatten()
                    .map(|x| x.episode),
            )
        }))
        .collect();
    let featured = featured_title(&popular_now, &titles);
    let rows: Vec<(RowKind, Vec<Uuid>)> = [
        (RowKind::ContinueWatching, continue_watching),
        (RowKind::RecentlyAdded, recently_added(&titles)),
        (RowKind::PopularNow, popular_now),
    ]
    .into_iter()
    .chain(
        genre_rows(&titles)
            .into_iter()
            .map(|(genre, ids)| (RowKind::Genre(genre), ids)),
    )
    .filter(|(_, ids)| !ids.is_empty())
    .collect();
    let shown: HashSet<Uuid> = rows
        .iter()
        .flat_map(|(_, ids)| ids.iter().copied())
        .chain(featured)
        .collect();
    let play_episode_id = featured
        .and_then(|id| show_play_target(show_episodes.get(&id)?.iter().copied()))
        .map(|x| x.episode.id);
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
                .chain(episode_tiles.iter().map(|x| x.season.show_id))
                .collect::<Vec<_>>(),
        ),
        &url_signer,
        now,
    )
    .await
    .map_err(Error::caused_by)?;
    let mut watches = movie_watches(
        &mut db_context,
        user_id,
        movies.iter().filter(|x| shown.contains(&x.id)),
    )
    .await
    .map_err(Error::caused_by)?;
    let media_files: HashMap<Uuid, MediaFile> = db_context
        .media_files()
        .filter(
            MediaFile::fields().id().in_list(
                episode_tiles
                    .iter()
                    .filter_map(|x| x.episode.media_file_id)
                    .collect::<Vec<_>>(),
            ),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| (x.id, x))
        .collect();
    let thumbs: HashMap<Uuid, Image> = db_context
        .artwork()
        .filter(
            Artwork::fields()
                .episode_id()
                .in_list(
                    episode_tiles
                        .iter()
                        .map(|x| x.episode.id)
                        .collect::<Vec<_>>(),
                )
                .and(Artwork::fields().kind().is_thumb()),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .iter()
        .filter_map(|x| Some((x.episode_id?, Image::new(x, &url_signer, now))))
        .collect();
    let show_of: HashMap<_, _> = shows.iter().map(|x| (x.id, x)).collect();
    let mut tiles: HashMap<Uuid, Tile> = episode_tiles
        .iter()
        .filter_map(|&x| {
            let show = show_of.get(&x.season.show_id)?;
            let tile = Tile::episode(
                show,
                x,
                show_images.get(&show.id).cloned().unwrap_or_default(),
                thumbs.get(&x.episode.id).cloned(),
                x.episode.media_file_id.and_then(|x| media_files.get(&x)),
            );
            Some((x.episode.id, tile))
        })
        .collect();
    let seasons_in_library: HashSet<Uuid> = episodes
        .iter()
        .filter(|x| x.in_library)
        .map(|x| x.season_id)
        .collect();
    let season_counts = season_counts(
        seasons
            .iter()
            .filter(|x| seasons_in_library.contains(&x.id)),
    );
    tiles.extend(
        movies
            .into_iter()
            .filter(|x| shown.contains(&x.id))
            .map(|movie| {
                let images = movie_images.remove(&movie.id).unwrap_or_default();
                let watch = watches.remove(&movie.id).unwrap_or_default();
                (movie.id, Tile::movie(movie, images, watch))
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
            ),
    );

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
    use crate::browsing::TileKind;
    use crate::browsing::remove_movie_from_continue_watching;
    use crate::browsing::remove_show_from_continue_watching;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::ports::persistence::DbScope;
    use crate::test::add_media_file;
    use crate::test::add_season;
    use crate::test::at;
    use crate::test::episode;
    use crate::test::mock_db_context;
    use crate::test::movie;
    use crate::test::show;
    use crate::test::stop_at;
    use crate::test::url_signer;
    use crate::test::watch_to_the_end;
    use domain::ArtworkKind;
    use domain::Show;
    use jiff::SignedDuration;
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

    async fn home(db_context: &DbContext) -> Output {
        home_for(db_context, Uuid::new_v4()).await
    }

    const NOW: &str = "2026-11-03T21:00:00Z";

    async fn home_for(db_context: &DbContext, user_id: Uuid) -> Output {
        let clock = FakeClock::new();
        clock.set(at(NOW));
        call(
            Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
                url_signer: url_signer(),
            },
            user_id,
        )
        .await
        .unwrap()
    }

    async fn add_file(db_context: &mut DbContext, episode_id: Uuid, duration_ms: i64) -> Uuid {
        let media_file_id = add_media_file(db_context, Some(duration_ms)).await;
        db_context
            .episodes()
            .filter(domain::Episode::fields().id().eq(episode_id))
            .update()
            .media_file_id(Some(media_file_id))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        media_file_id
    }

    async fn movie_with_file(db_context: &mut DbContext, movie: <Movie as Model>::Create) -> Uuid {
        let media_file_id = add_media_file(db_context, Some(2_400_000)).await;
        movie
            .media_file_id(Some(media_file_id))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        media_file_id
    }

    fn row<'a>(output: &'a Output, kind: &RowKind) -> &'a [Tile] {
        output
            .rows
            .iter()
            .find(|x| &x.kind == kind)
            .map(|x| x.tiles.as_slice())
            .unwrap_or_default()
    }

    type ContinueWatchingTile<'a> = (&'a str, Option<(i64, i64)>, Option<f64>);

    fn continue_watching(output: &Output) -> Vec<ContinueWatchingTile<'_>> {
        row(output, &RowKind::ContinueWatching)
            .iter()
            .map(|tile| match &tile.kind {
                TileKind::Episode {
                    show,
                    season_number,
                    episode_number,
                    ..
                } => (
                    show.title.as_str(),
                    Some((*season_number, *episode_number)),
                    tile.progress,
                ),
                _ => (tile.title.as_str(), None, tile.progress),
            })
            .collect()
    }

    fn titles(tiles: &[Tile]) -> Vec<&str> {
        tiles.iter().map(|x| x.title.as_str()).collect()
    }

    fn genres(names: &[&str]) -> Vec<String> {
        names.iter().map(|x| (*x).to_owned()).collect()
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

    #[tokio::test]
    async fn the_featured_show_plays_and_the_tiles_show_progress_from_the_users_watch_state() {
        let mut db_context = mock_db_context().await;
        let arrival_file = add_media_file(&db_context, Some(2_400_000)).await;
        movie(&db_context, "Arrival")
            .added_at(Some(at("2026-10-29T08:00:00Z")))
            .media_file_id(Some(arrival_file))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let severance = show(&db_context, "Severance");
        let dated = |number| Episode {
            number,
            air_date: Some(date(2022, 2, 18)),
            ..EPISODE
        };
        let episodes = add_show(&mut db_context, severance, &[dated(1), dated(2)]).await;
        let first_file = add_media_file(&db_context, None).await;
        db_context
            .episodes()
            .filter(domain::Episode::fields().id().eq(episodes[&(1, 1)]))
            .update()
            .media_file_id(Some(first_file))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let noor = Uuid::new_v4();
        watch_to_the_end(&db_context, noor, first_file, "2026-11-02T20:00:00Z").await;
        stop_at(
            &db_context,
            noor,
            arrival_file,
            SignedDuration::from_mins(10),
            "2026-11-01T20:00:00Z",
        )
        .await;

        let output = home_for(&db_context, noor).await;

        let featured = output.featured.as_ref().unwrap();
        assert_eq!(featured.tile.title, "Severance");
        assert_eq!(featured.play_episode_id, Some(episodes[&(1, 2)]));
        let arrival = &row(&output, &RowKind::RecentlyAdded)[0];
        assert_eq!(
            (arrival.title.as_str(), arrival.progress),
            ("Arrival", Some(0.25))
        );
    }

    #[tokio::test]
    async fn continue_watching_holds_resume_positions_and_next_up_episodes_newest_first_with_one_tile_per_show()
     {
        let mut db_context = mock_db_context().await;
        let noor = Uuid::new_v4();
        let sam = Uuid::new_v4();
        let arrival = movie(&db_context, "Arrival");
        let arrival_file = movie_with_file(&mut db_context, arrival).await;
        let gone = movie(&db_context, "Gone").in_library(false);
        let gone_file = movie_with_file(&mut db_context, gone).await;
        let dune = movie(&db_context, "Dune");
        let dune_file = movie_with_file(&mut db_context, dune).await;
        let severance = show(&db_context, "Severance");
        let severance = add_show(
            &mut db_context,
            severance,
            &[
                EPISODE,
                Episode {
                    number: 2,
                    ..EPISODE
                },
                Episode {
                    number: 3,
                    ..EPISODE
                },
                Episode {
                    number: 4,
                    in_library: false,
                    ..EPISODE
                },
            ],
        )
        .await;
        let andor = show(&db_context, "Andor");
        let andor = add_show(
            &mut db_context,
            andor,
            &[
                EPISODE,
                Episode {
                    number: 2,
                    ..EPISODE
                },
            ],
        )
        .await;
        let mut files = HashMap::new();
        for id in severance.values().chain(andor.values()) {
            files.insert(*id, add_file(&mut db_context, *id, 3_000_000).await);
        }
        let minutes = SignedDuration::from_mins;
        stop_at(
            &db_context,
            noor,
            files[&severance[&(1, 1)]],
            minutes(10),
            "2026-10-30T20:00:00Z",
        )
        .await;
        stop_at(
            &db_context,
            noor,
            files[&severance[&(1, 2)]],
            minutes(15),
            "2026-11-02T20:00:00Z",
        )
        .await;
        stop_at(
            &db_context,
            noor,
            files[&severance[&(1, 4)]],
            minutes(20),
            "2026-11-03T20:00:00Z",
        )
        .await;
        stop_at(
            &db_context,
            noor,
            arrival_file,
            minutes(10),
            "2026-11-01T20:00:00Z",
        )
        .await;
        stop_at(
            &db_context,
            noor,
            gone_file,
            minutes(10),
            "2026-11-03T20:00:00Z",
        )
        .await;
        watch_to_the_end(
            &db_context,
            noor,
            files[&andor[&(1, 1)]],
            "2026-10-31T20:00:00Z",
        )
        .await;
        stop_at(
            &db_context,
            sam,
            dune_file,
            minutes(10),
            "2026-11-03T20:00:00Z",
        )
        .await;

        let output = home_for(&db_context, noor).await;

        assert_eq!(
            continue_watching(&output),
            [
                ("Severance", Some((1, 2)), Some(0.3)),
                ("Arrival", None, Some(0.25)),
                ("Andor", Some((1, 2)), None),
            ]
        );
    }

    #[tokio::test]
    async fn a_next_up_tile_moves_to_the_front_of_continue_watching_when_its_episode_is_added() {
        let mut db_context = mock_db_context().await;
        let noor = Uuid::new_v4();
        let arrival = movie(&db_context, "Arrival");
        let arrival_file = movie_with_file(&mut db_context, arrival).await;
        let andor = show(&db_context, "Andor");
        let andor = add_show(&mut db_context, andor, &[EPISODE]).await;
        let first_file = add_file(&mut db_context, andor[&(1, 1)], 3_000_000).await;
        watch_to_the_end(&db_context, noor, first_file, "2026-10-31T20:00:00Z").await;
        stop_at(
            &db_context,
            noor,
            arrival_file,
            SignedDuration::from_mins(10),
            "2026-11-01T20:00:00Z",
        )
        .await;
        let caught_up = home_for(&db_context, noor).await;
        let season_id = db_context
            .episodes()
            .filter(domain::Episode::fields().id().eq(andor[&(1, 1)]))
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap()
            .season_id;
        episode(&db_context, season_id)
            .number(2)
            .added_at(Some(at("2026-11-02T08:00:00Z")))
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        let output = home_for(&db_context, noor).await;

        assert_eq!(
            continue_watching(&caught_up),
            [("Arrival", None, Some(0.25))]
        );
        assert_eq!(
            continue_watching(&output),
            [("Andor", Some((1, 2)), None), ("Arrival", None, Some(0.25))]
        );
    }

    #[tokio::test]
    async fn an_episode_tile_carries_the_episode_thumb_and_runtime_and_the_shows_logo_and_fanart() {
        let mut db_context = mock_db_context().await;
        let noor = Uuid::new_v4();
        let severance = show(&db_context, "Severance").certification(Some("TV-MA".to_owned()));
        let severance = severance.exec(db_context.executor_mut()).await.unwrap();
        let season_id = add_season(&mut db_context, severance.id, 2).await;
        let episode = episode(&db_context, season_id)
            .number(4)
            .title("Woe's Hollow")
            .overview(Some("The team goes on an outing.".to_owned()))
            .air_date(Some(date(2025, 2, 7)))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let file = add_file(&mut db_context, episode.id, 3_420_000).await;
        let mut artwork = Vec::new();
        for (kind, episode_id) in [
            (ArtworkKind::Fanart, None),
            (ArtworkKind::Clearlogo, None),
            (ArtworkKind::Thumb, Some(episode.id)),
        ] {
            let id = db_context
                .artwork()
                .create()
                .show_id(Some(severance.id))
                .episode_id(episode_id)
                .kind(kind)
                .path(format!("/media/TV/Severance/{kind:?}"))
                .modified_at(at("2026-10-01T08:00:00Z"))
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .id;
            artwork.push(id);
        }
        stop_at(
            &db_context,
            noor,
            file,
            SignedDuration::from_mins(10),
            "2026-11-02T20:00:00Z",
        )
        .await;

        let output = home_for(&db_context, noor).await;

        let tile = &row(&output, &RowKind::ContinueWatching)[0];
        let TileKind::Episode {
            runtime_minutes,
            show,
            season_number,
            episode_number,
        } = &tile.kind
        else {
            panic!("{tile:?} is not an episode tile");
        };
        assert_eq!(
            (
                tile.id,
                tile.title.as_str(),
                tile.year,
                tile.plot.as_deref()
            ),
            (
                episode.id,
                "Woe's Hollow",
                Some(2025),
                Some("The team goes on an outing.")
            )
        );
        assert_eq!(tile.certification.as_deref(), Some("TV-MA"));
        assert_eq!(
            (*runtime_minutes, *season_number, *episode_number),
            (Some(57), 2, 4)
        );
        assert_eq!(
            (
                show.id,
                show.title.as_str(),
                show.logo.as_ref().map(|x| x.id)
            ),
            (severance.id, "Severance", Some(artwork[1]))
        );
        assert_eq!(
            (
                tile.images.thumb.as_ref().map(|x| x.id),
                tile.images.fanart.as_ref().map(|x| x.id),
                tile.images.logo.as_ref().map(|x| x.id),
            ),
            (Some(artwork[2]), Some(artwork[0]), None)
        );
    }

    #[tokio::test]
    async fn popular_now_ranks_titles_in_the_library_by_how_many_users_played_them_in_the_last_14_days()
     {
        let mut db_context = mock_db_context().await;
        let [noor, sam, ines] = [(); 3].map(|()| Uuid::new_v4());
        let mut files = HashMap::new();
        for (title, in_library) in [
            ("Arrival", true),
            ("Dune", true),
            ("Gone", false),
            ("Sicario", true),
        ] {
            let movie = movie(&db_context, title).in_library(in_library);
            files.insert(title, movie_with_file(&mut db_context, movie).await);
        }
        let severance = show(&db_context, "Severance");
        let severance = add_show(
            &mut db_context,
            severance,
            &[
                EPISODE,
                Episode {
                    number: 2,
                    ..EPISODE
                },
            ],
        )
        .await;
        let first = add_file(&mut db_context, severance[&(1, 1)], 3_000_000).await;
        let second = add_file(&mut db_context, severance[&(1, 2)], 3_000_000).await;
        for (user_id, file, reported_at) in [
            (noor, files["Arrival"], "2026-11-01T20:00:00Z"),
            (sam, files["Arrival"], "2026-11-01T20:00:00Z"),
            (ines, files["Arrival"], "2026-11-01T20:00:00Z"),
            (noor, first, "2026-11-02T20:00:00Z"),
            (noor, second, "2026-11-02T21:00:00Z"),
            (sam, first, "2026-10-21T20:00:00Z"),
            (noor, files["Dune"], "2026-11-03T20:00:00Z"),
            (noor, files["Gone"], "2026-11-03T20:00:00Z"),
            (sam, files["Gone"], "2026-11-03T20:00:00Z"),
            (ines, files["Gone"], "2026-11-03T20:00:00Z"),
            (noor, files["Sicario"], "2026-10-20T20:00:00Z"),
        ] {
            watch_to_the_end(&db_context, user_id, file, reported_at).await;
        }

        let output = home_for(&db_context, ines).await;

        assert_eq!(
            titles(row(&output, &RowKind::PopularNow)),
            ["Arrival", "Severance", "Dune"]
        );
    }

    #[tokio::test]
    async fn the_rows_are_continue_watching_recently_added_popular_now_then_the_genre_rows() {
        let mut db_context = mock_db_context().await;
        let noor = Uuid::new_v4();
        let arrival = movie(&db_context, "Arrival")
            .genres(genres(&["Drama"]))
            .added_at(Some(at("2026-10-29T08:00:00Z")));
        let arrival_file = movie_with_file(&mut db_context, arrival).await;
        stop_at(
            &db_context,
            noor,
            arrival_file,
            SignedDuration::from_mins(10),
            "2026-11-01T20:00:00Z",
        )
        .await;

        let output = home_for(&db_context, noor).await;

        assert_eq!(
            output.rows.iter().map(|x| &x.kind).collect::<Vec<_>>(),
            [
                &RowKind::ContinueWatching,
                &RowKind::RecentlyAdded,
                &RowKind::PopularNow,
                &RowKind::Genre("Drama".to_owned()),
            ]
        );
    }

    async fn remove_movie(db_context: &DbContext, user_id: Uuid, movie_id: Uuid, now: &str) {
        let clock = FakeClock::new();
        clock.set(at(now));
        remove_movie_from_continue_watching::call(
            remove_movie_from_continue_watching::Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
            },
            user_id,
            movie_id,
        )
        .await
        .unwrap();
    }

    async fn remove_show(db_context: &DbContext, user_id: Uuid, show_id: Uuid, now: &str) {
        let clock = FakeClock::new();
        clock.set(at(now));
        remove_show_from_continue_watching::call(
            remove_show_from_continue_watching::Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
            },
            user_id,
            show_id,
        )
        .await
        .unwrap();
    }

    async fn show_id_of(db_context: &DbContext, episode_id: Uuid) -> Uuid {
        let mut db_context = db_context.clone();
        let episode = db_context
            .episodes()
            .filter(domain::Episode::fields().id().eq(episode_id))
            .include(domain::Episode::fields().season())
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap();
        episode.season.get().show_id
    }

    #[tokio::test]
    async fn removing_a_movie_and_a_show_leaves_them_out_of_the_users_continue_watching_only() {
        let mut db_context = mock_db_context().await;
        let [noor, sam] = [(); 2].map(|()| Uuid::new_v4());
        let arrival_file = add_media_file(&db_context, Some(2_400_000)).await;
        let arrival_id = movie(&db_context, "Arrival")
            .media_file_id(Some(arrival_file))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        let severance = show(&db_context, "Severance");
        let severance = add_show(&mut db_context, severance, &[EPISODE]).await;
        let severance_file = add_file(&mut db_context, severance[&(1, 1)], 3_000_000).await;
        for user_id in [noor, sam] {
            for file in [arrival_file, severance_file] {
                stop_at(
                    &db_context,
                    user_id,
                    file,
                    SignedDuration::from_mins(10),
                    "2026-11-01T20:00:00Z",
                )
                .await;
            }
        }

        remove_movie(&db_context, noor, arrival_id, "2026-11-02T20:00:00Z").await;
        let severance_id = show_id_of(&db_context, severance[&(1, 1)]).await;
        remove_show(&db_context, noor, severance_id, "2026-11-02T20:00:00Z").await;

        assert_eq!(continue_watching(&home_for(&db_context, noor).await), []);
        assert_eq!(
            continue_watching(&home_for(&db_context, sam).await),
            [
                ("Arrival", None, Some(0.25)),
                ("Severance", Some((1, 1)), Some(0.2)),
            ]
        );
    }

    #[tokio::test]
    async fn a_play_after_the_removal_brings_a_title_back_but_a_new_episode_does_not() {
        let mut db_context = mock_db_context().await;
        let noor = Uuid::new_v4();
        let arrival_file = add_media_file(&db_context, Some(2_400_000)).await;
        let arrival_id = movie(&db_context, "Arrival")
            .media_file_id(Some(arrival_file))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        let andor = show(&db_context, "Andor");
        let andor = add_show(&mut db_context, andor, &[EPISODE]).await;
        let first_file = add_file(&mut db_context, andor[&(1, 1)], 3_000_000).await;
        let ten_minutes = SignedDuration::from_mins(10);
        stop_at(
            &db_context,
            noor,
            arrival_file,
            ten_minutes,
            "2026-10-30T20:00:00Z",
        )
        .await;
        watch_to_the_end(&db_context, noor, first_file, "2026-10-30T21:00:00Z").await;
        remove_movie(&db_context, noor, arrival_id, "2026-10-31T20:00:00Z").await;
        let andor_id = show_id_of(&db_context, andor[&(1, 1)]).await;
        remove_show(&db_context, noor, andor_id, "2026-10-31T20:00:00Z").await;

        stop_at(
            &db_context,
            noor,
            arrival_file,
            ten_minutes,
            "2026-11-01T20:00:00Z",
        )
        .await;
        let season_id = db_context
            .episodes()
            .filter(domain::Episode::fields().id().eq(andor[&(1, 1)]))
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap()
            .season_id;
        episode(&db_context, season_id)
            .number(2)
            .added_at(Some(at("2026-11-02T08:00:00Z")))
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        let output = home_for(&db_context, noor).await;

        assert_eq!(continue_watching(&output), [("Arrival", None, Some(0.25))]);
    }

    async fn leave_library(db_context: &mut DbContext, episode_id: Uuid) {
        db_context
            .episodes()
            .filter(domain::Episode::fields().id().eq(episode_id))
            .update()
            .in_library(false)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }

    async fn add_show_with_files(
        db_context: &mut DbContext,
        title: &str,
        episodes: i64,
    ) -> (HashMap<(i64, i64), Uuid>, HashMap<i64, Uuid>) {
        let show = show(db_context, title);
        let episodes: Vec<_> = (1..=episodes)
            .map(|number| Episode { number, ..EPISODE })
            .collect();
        let ids = add_show(db_context, show, &episodes).await;
        let mut files = HashMap::new();
        for (&(_, number), &id) in &ids {
            files.insert(number, add_file(db_context, id, 3_000_000).await);
        }
        (ids, files)
    }

    #[tokio::test]
    async fn next_up_follows_the_latest_watched_episode_even_when_it_has_left_the_library() {
        let mut db_context = mock_db_context().await;
        let noor = Uuid::new_v4();
        let (episodes, files) = add_show_with_files(&mut db_context, "Severance", 4).await;
        watch_to_the_end(&db_context, noor, files[&1], "2026-10-20T20:00:00Z").await;
        watch_to_the_end(&db_context, noor, files[&3], "2026-10-25T20:00:00Z").await;
        leave_library(&mut db_context, episodes[&(1, 3)]).await;

        let output = home_for(&db_context, noor).await;

        assert_eq!(
            continue_watching(&output),
            [("Severance", Some((1, 4)), None)]
        );
    }

    #[tokio::test]
    async fn a_play_of_an_episode_that_has_left_the_library_brings_a_removed_show_back() {
        let mut db_context = mock_db_context().await;
        let noor = Uuid::new_v4();
        let (episodes, files) = add_show_with_files(&mut db_context, "Andor", 3).await;
        watch_to_the_end(&db_context, noor, files[&1], "2026-10-20T20:00:00Z").await;
        let andor_id = show_id_of(&db_context, episodes[&(1, 1)]).await;
        remove_show(&db_context, noor, andor_id, "2026-10-21T20:00:00Z").await;
        watch_to_the_end(&db_context, noor, files[&2], "2026-10-22T20:00:00Z").await;
        leave_library(&mut db_context, episodes[&(1, 2)]).await;

        let output = home_for(&db_context, noor).await;

        assert_eq!(continue_watching(&output), [("Andor", Some((1, 3)), None)]);
    }

    #[tokio::test]
    async fn popular_now_counts_plays_of_episodes_that_have_left_the_library() {
        let mut db_context = mock_db_context().await;
        let (episodes, files) = add_show_with_files(&mut db_context, "Severance", 2).await;
        watch_to_the_end(
            &db_context,
            Uuid::new_v4(),
            files[&1],
            "2026-11-01T20:00:00Z",
        )
        .await;
        leave_library(&mut db_context, episodes[&(1, 1)]).await;

        let output = home(&db_context).await;

        assert_eq!(titles(row(&output, &RowKind::PopularNow)), ["Severance"]);
    }
}
