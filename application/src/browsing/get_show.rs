mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::CastMember;
pub use output::Output;
pub use output::Play;
pub use output::Season;
pub use output::Show;

use crate::browsing::more_like_this;
use crate::browsing::season_counts;
use crate::browsing::title_images;
use crate::ports::persistence::DbScope;
use domain::Actor;
use domain::Artwork;
use domain::Episode;
use domain::first_episode;
use std::collections::HashMap;
use uuid::Uuid;

const CAST_SIZE: usize = 10;

pub async fn call(dependencies: impl Into<Dependencies>, show_id: Uuid) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        url_signer,
    } = dependencies.into();
    let Some(show) = db_context
        .shows()
        .filter(
            domain::Show::fields()
                .id()
                .eq(show_id)
                .and(domain::Show::in_library_filter()),
        )
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(Output::NotFound);
    };
    let mut seasons = db_context
        .seasons()
        .filter(
            domain::Season::fields()
                .show_id()
                .eq(show.id)
                .and(domain::Season::in_library_filter()),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    seasons.sort_by_key(domain::Season::listing_order);
    let episodes = db_context
        .episodes()
        .filter(
            Episode::fields()
                .season_id()
                .in_list(seasons.iter().map(|x| x.id).collect::<Vec<_>>())
                .and(Episode::in_library_filter()),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let season_of: HashMap<_, _> = seasons.iter().map(|x| (x.id, x)).collect();
    let Some((play_season, play_episode)) = first_episode(
        episodes
            .iter()
            .filter_map(|x| Some((*season_of.get(&x.season_id)?, x))),
    ) else {
        return Ok(Output::NotFound);
    };
    let play = Play {
        episode_id: play_episode.id,
        episode_number: play_episode.number,
        episode_title: play_episode.title.clone(),
        season_id: play_season.id,
        season_number: play_season.number,
    };
    let mut episode_counts = HashMap::<Uuid, i64>::new();
    for episode in &episodes {
        *episode_counts.entry(episode.season_id).or_default() += 1;
    }
    let mut actors = db_context
        .actors()
        .filter(Actor::fields().show_id().eq(show.id))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    actors.sort_by_key(|x| (x.order.is_none(), x.order));
    let images = title_images(
        &mut db_context,
        |x| x.show_id,
        Artwork::fields().show_id().eq(show.id),
        &url_signer,
        clock.call(),
    )
    .await
    .map_err(Error::caused_by)?
    .remove(&show.id)
    .unwrap_or_default();
    let more_like_this = more_like_this(
        &mut db_context,
        &show.genres,
        show.id,
        &url_signer,
        clock.call(),
    )
    .await
    .map_err(Error::caused_by)?;
    Ok(Output::Show(Box::new(Show {
        id: show.id,
        season_count: season_counts(&seasons)
            .get(&show.id)
            .copied()
            .unwrap_or_default(),
        title: show.title,
        year: show.year,
        certification: show.certification,
        status: show.status,
        network: show.network,
        genres: show.genres,
        rating: show.rating,
        plot: show.overview,
        cast: actors
            .into_iter()
            .take(CAST_SIZE)
            .map(|x| CastMember {
                name: x.name,
                role: x.role,
            })
            .collect(),
        images,
        seasons: seasons
            .iter()
            .map(|x| Season {
                id: x.id,
                number: x.number,
                name: x.name(),
                episode_count: episode_counts.get(&x.id).copied().unwrap_or_default(),
            })
            .collect(),
        play,
        more_like_this,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browsing::Image;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::ports::persistence::DbScope;
    use crate::test::add_season;
    use crate::test::episode;
    use crate::test::mock_db_context;
    use crate::test::show;
    use crate::test::url_signer;
    use domain::ArtworkKind;
    use jiff::Timestamp;
    use std::collections::HashMap;
    use test_fixtures::FakeClock;

    fn dependencies(db_context: &DbContext) -> Dependencies {
        Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(FakeClock::new().reader()),
            url_signer: url_signer(),
        }
    }

    async fn get(db_context: &DbContext, show_id: Uuid) -> Output {
        call(dependencies(db_context), show_id).await.unwrap()
    }

    async fn get_show(db_context: &DbContext, show_id: Uuid) -> Show {
        match get(db_context, show_id).await {
            Output::Show(show) => *show,
            Output::NotFound => panic!("the show is not found"),
        }
    }

    async fn add_show(
        db_context: &mut DbContext,
        title: &str,
        episodes: &[(i64, i64, bool)],
    ) -> (Uuid, HashMap<(i64, i64), Uuid>) {
        let show = show(db_context, title)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let mut seasons = HashMap::new();
        let mut ids = HashMap::new();
        for &(season, number, visible) in episodes {
            let season_id = match seasons.get(&season) {
                Some(id) => *id,
                None => {
                    let id = add_season(db_context, show.id, season).await;
                    seasons.insert(season, id);
                    id
                }
            };
            let episode = episode(db_context, season_id)
                .number(number)
                .visible(visible)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            ids.insert((season, number), episode.id);
        }
        (show.id, ids)
    }

    fn widths(image: Option<&Image>) -> Option<(Uuid, Vec<i64>)> {
        image.map(|x| (x.id, x.urls.iter().map(|x| x.width).collect()))
    }

    #[tokio::test]
    async fn a_show_page_carries_its_metadata_first_10_cast_members_and_artwork() {
        let mut db_context = mock_db_context().await;
        let severance = show(&db_context, "Severance")
            .certification(Some("TV-MA".to_owned()))
            .status(Some("Continuing".to_owned()))
            .network(Some("Apple TV+".to_owned()))
            .genres(vec!["Drama".to_owned(), "Mystery".to_owned()])
            .rating(Some(8.7))
            .overview(Some("Office workers have their memories split.".to_owned()))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        for number in [1, 2, 0] {
            let season_id = add_season(&mut db_context, severance.id, number).await;
            episode(&db_context, season_id)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        for order in (0..11).rev() {
            db_context
                .actors()
                .create()
                .show_id(Some(severance.id))
                .name(format!("Actor {order:02}"))
                .role((order != 1).then(|| format!("Role {order:02}")))
                .order(Some(order))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        let mut artwork = Vec::new();
        for kind in [
            ArtworkKind::Fanart,
            ArtworkKind::Clearlogo,
            ArtworkKind::Poster,
        ] {
            let id = db_context
                .artwork()
                .create()
                .show_id(Some(severance.id))
                .kind(kind)
                .path(format!("/media/TV/Severance/{kind:?}"))
                .modified_at(Timestamp::UNIX_EPOCH)
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .id;
            artwork.push(id);
        }

        let page = get_show(&db_context, severance.id).await;

        assert_eq!(page.id, severance.id);
        assert_eq!(page.title, "Severance");
        assert_eq!(page.year, Some(2022));
        assert_eq!(page.certification.as_deref(), Some("TV-MA"));
        assert_eq!(page.status.as_deref(), Some("Continuing"));
        assert_eq!(page.network.as_deref(), Some("Apple TV+"));
        assert_eq!(page.season_count, 2);
        assert_eq!(page.genres, ["Drama", "Mystery"]);
        assert_eq!(page.rating, Some(8.7));
        assert_eq!(
            page.plot.as_deref(),
            Some("Office workers have their memories split.")
        );
        assert_eq!(
            page.cast,
            (0..10)
                .map(|x| CastMember {
                    name: format!("Actor {x:02}"),
                    role: (x != 1).then(|| format!("Role {x:02}")),
                })
                .collect::<Vec<_>>()
        );
        assert_eq!(
            widths(page.images.fanart.as_ref()),
            Some((artwork[0], vec![480, 960, 1280, 1920]))
        );
        assert_eq!(
            widths(page.images.logo.as_ref()),
            Some((artwork[1], vec![300, 600]))
        );
    }

    #[tokio::test]
    async fn the_seasons_with_visible_episodes_are_listed_with_their_episode_counts_and_specials_last()
     {
        let mut db_context = mock_db_context().await;
        let (severance, _) = add_show(
            &mut db_context,
            "Severance",
            &[
                (0, 1, true),
                (2, 1, true),
                (2, 2, false),
                (1, 1, true),
                (1, 2, true),
                (1, 3, true),
                (3, 1, false),
            ],
        )
        .await;

        let page = get_show(&db_context, severance).await;

        assert_eq!(
            page.seasons
                .iter()
                .map(|x| (x.number, x.name.as_str(), x.episode_count))
                .collect::<Vec<_>>(),
            [(1, "Season 1", 3), (2, "Season 2", 1), (0, "Specials", 1)]
        );
        assert_eq!(page.season_count, 2);
    }

    #[tokio::test]
    async fn with_no_watch_state_play_starts_the_first_visible_episode_of_the_first_numbered_season()
     {
        let mut db_context = mock_db_context().await;
        let (severance, episodes) = add_show(
            &mut db_context,
            "Severance",
            &[(0, 1, true), (1, 1, false), (1, 2, true), (2, 1, true)],
        )
        .await;

        let page = get_show(&db_context, severance).await;

        let season_one = page.seasons.iter().find(|x| x.number == 1).unwrap().id;
        assert_eq!(
            (
                page.play.episode_id,
                page.play.season_id,
                page.play.season_number,
                page.play.episode_number
            ),
            (episodes[&(1, 2)], season_one, 1, 2)
        );
        assert_eq!(page.play.episode_title, "Good News About Hell");
    }

    #[tokio::test]
    async fn a_show_with_only_specials_plays_its_first_special() {
        let mut db_context = mock_db_context().await;
        let (show_id, episodes) = add_show(
            &mut db_context,
            "Specials Only",
            &[(0, 2, true), (0, 1, true), (1, 1, false)],
        )
        .await;

        let page = get_show(&db_context, show_id).await;

        assert_eq!(
            (page.play.episode_id, page.play.season_number),
            (episodes[&(0, 1)], 0)
        );
        assert_eq!(page.season_count, 0);
    }

    #[tokio::test]
    async fn more_like_this_leaves_out_the_show_itself() {
        let mut db_context = mock_db_context().await;
        for title in ["Severance", "Andor"] {
            let show = show(&db_context, title)
                .genres(vec!["Drama".to_owned()])
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            let season_id = add_season(&mut db_context, show.id, 1).await;
            episode(&db_context, season_id)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        let severance = db_context
            .shows()
            .filter(domain::Show::fields().title().eq("Severance"))
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap();

        let page = get_show(&db_context, severance.id).await;

        assert_eq!(
            page.more_like_this
                .iter()
                .map(|x| x.title.as_str())
                .collect::<Vec<_>>(),
            ["Andor"]
        );
    }

    #[tokio::test]
    async fn an_unknown_show_or_one_without_visible_episodes_is_not_found() {
        let mut db_context = mock_db_context().await;
        let (hidden, _) = add_show(&mut db_context, "Gone", &[(1, 1, false)]).await;
        let empty = show(&db_context, "Empty")
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        for show_id in [hidden, empty.id, Uuid::new_v4()] {
            let output = get(&db_context, show_id).await;

            assert!(matches!(output, Output::NotFound), "{output:?}");
        }
    }
}
