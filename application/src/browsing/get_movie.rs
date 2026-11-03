mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::CastMember;
pub use output::Movie;
pub use output::Output;

use crate::browsing::more_like_this;
use crate::browsing::movie_watches;
use crate::browsing::title_images;
use crate::ports::persistence::DbScope;
use domain::Actor;
use domain::Artwork;
use uuid::Uuid;

const CAST_SIZE: usize = 10;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    movie_id: Uuid,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        url_signer,
    } = dependencies.into();
    let Some(movie) = db_context
        .movies()
        .filter(
            domain::Movie::fields()
                .id()
                .eq(movie_id)
                .and(domain::Movie::in_library_filter()),
        )
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(Output::NotFound);
    };
    let mut actors = db_context
        .actors()
        .filter(Actor::fields().movie_id().eq(movie.id))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    actors.sort_by_key(Actor::listing_order);
    let images = title_images(
        &mut db_context,
        |x| x.movie_id,
        Artwork::fields().movie_id().eq(movie.id),
        &url_signer,
        clock.call(),
    )
    .await
    .map_err(Error::caused_by)?
    .remove(&movie.id)
    .unwrap_or_default();
    let watch = movie_watches(&mut db_context, user_id, [&movie])
        .await
        .map_err(Error::caused_by)?
        .remove(&movie.id)
        .unwrap_or_default();
    let more_like_this = more_like_this(
        &mut db_context,
        user_id,
        &movie.genres,
        movie.id,
        &url_signer,
        clock.call(),
    )
    .await
    .map_err(Error::caused_by)?;
    Ok(Output::Movie(Box::new(Movie {
        id: movie.id,
        title: movie.title,
        year: movie.year,
        certification: movie.certification,
        runtime_minutes: movie.runtime_minutes,
        genres: movie.genres,
        rating: movie.rating,
        plot: movie.overview,
        studio: movie.studio,
        original_title: movie.original_title,
        directors: movie.directors,
        media_file_id: movie.media_file_id,
        cast: actors
            .into_iter()
            .take(CAST_SIZE)
            .map(|x| CastMember {
                name: x.name,
                role: x.role,
            })
            .collect(),
        images,
        resume_position_seconds: watch.state.resume_position_seconds,
        progress: watch.progress,
        minutes_left: watch.minutes_left,
        watched: watch.state.watched,
        more_like_this,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browsing::Image;
    use crate::browsing::TileKind;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::ports::persistence::DbScope;
    use crate::test::add_media_file;
    use crate::test::add_season;
    use crate::test::episode;
    use crate::test::mock_db_context;
    use crate::test::movie;
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

    async fn get(db_context: &DbContext, movie_id: Uuid) -> Output {
        call(dependencies(db_context), Uuid::new_v4(), movie_id)
            .await
            .unwrap()
    }

    async fn add_artwork(db_context: &mut DbContext, movie_id: Uuid, kind: ArtworkKind) -> Uuid {
        db_context
            .artwork()
            .create()
            .movie_id(Some(movie_id))
            .kind(kind)
            .path(format!("/mnt/media/movies/Amélie (2001)/{kind:?}"))
            .modified_at(Timestamp::UNIX_EPOCH)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    fn widths(image: Option<&Image>) -> Option<(Uuid, Vec<i64>)> {
        image.map(|x| (x.id, x.urls.iter().map(|x| x.width).collect()))
    }

    #[tokio::test]
    async fn a_movie_page_carries_its_metadata_director_first_10_cast_members_and_artwork() {
        let mut db_context = mock_db_context().await;
        let amelie = movie(&db_context, "Amélie")
            .original_title(Some("Le Fabuleux Destin d'Amélie Poulain".to_owned()))
            .year(2001)
            .certification(Some("R".to_owned()))
            .runtime_minutes(Some(122))
            .genres(vec!["Comedy".to_owned(), "Romance".to_owned()])
            .rating(Some(7.9))
            .overview(Some(
                "A shy waitress decides to change the lives of those around her.".to_owned(),
            ))
            .studio(Some("UGC".to_owned()))
            .directors(vec!["Jean-Pierre Jeunet".to_owned()])
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        for order in (0..11).rev() {
            db_context
                .actors()
                .create()
                .movie_id(Some(amelie.id))
                .name(format!("Actor {order:02}"))
                .role((order != 1).then(|| format!("Role {order:02}")))
                .order(Some(order))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        let fanart = add_artwork(&mut db_context, amelie.id, ArtworkKind::Fanart).await;
        let logo = add_artwork(&mut db_context, amelie.id, ArtworkKind::Clearlogo).await;
        add_artwork(&mut db_context, amelie.id, ArtworkKind::Poster).await;

        let Output::Movie(page) = get(&db_context, amelie.id).await else {
            panic!("the movie is not found");
        };

        assert_eq!(page.id, amelie.id);
        assert_eq!(page.title, "Amélie");
        assert_eq!(
            page.original_title.as_deref(),
            Some("Le Fabuleux Destin d'Amélie Poulain")
        );
        assert_eq!(page.year, 2001);
        assert_eq!(page.certification.as_deref(), Some("R"));
        assert_eq!(page.runtime_minutes, Some(122));
        assert_eq!(page.genres, ["Comedy", "Romance"]);
        assert_eq!(page.rating, Some(7.9));
        assert_eq!(
            page.plot.as_deref(),
            Some("A shy waitress decides to change the lives of those around her.")
        );
        assert_eq!(page.studio.as_deref(), Some("UGC"));
        assert_eq!(page.directors, ["Jean-Pierre Jeunet"]);
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
            Some((fanart, vec![480, 960, 1280, 1920]))
        );
        assert_eq!(
            widths(page.images.logo.as_ref()),
            Some((logo, vec![300, 600]))
        );
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|x| (*x).to_owned()).collect()
    }

    async fn add_show(db_context: &mut DbContext, title: &str, genres: &[&str], in_library: bool) {
        let show = show(db_context, title)
            .genres(names(genres))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let season_id = add_season(db_context, show.id, 1).await;
        episode(db_context, season_id)
            .in_library(in_library)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }

    fn more_like_this(output: &Output) -> Vec<&str> {
        let Output::Movie(page) = output else {
            panic!("the movie is not found");
        };
        page.more_like_this
            .iter()
            .map(|x| x.title.as_str())
            .collect()
    }

    #[tokio::test]
    async fn more_like_this_holds_the_movies_and_shows_in_the_library_that_share_a_genre() {
        let mut db_context = mock_db_context().await;
        let arrival = movie(&db_context, "Arrival")
            .genres(names(&["Drama", "Science Fiction"]))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        for (title, in_library) in [("Dune", true), ("Gone", false)] {
            movie(&db_context, title)
                .genres(names(&["Science Fiction"]))
                .in_library(in_library)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        add_show(&mut db_context, "Severance", &["Drama"], true).await;
        add_show(&mut db_context, "Gone", &["Drama"], false).await;

        let output = get(&db_context, arrival.id).await;

        assert_eq!(more_like_this(&output), ["Dune", "Severance"]);
        let Output::Movie(page) = output else {
            unreachable!()
        };
        assert!(
            matches!(page.more_like_this[0].kind, TileKind::Movie { .. }),
            "{:?}",
            page.more_like_this[0].kind
        );
        assert!(
            matches!(
                page.more_like_this[1].kind,
                TileKind::Show { season_count: 1 }
            ),
            "{:?}",
            page.more_like_this[1].kind
        );
    }

    #[tokio::test]
    async fn each_user_sees_their_own_resume_position_progress_minutes_left_and_watched_state() {
        let mut db_context = mock_db_context().await;
        let arrival_file = add_media_file(&db_context, Some(2_400_000)).await;
        let arrival = movie(&db_context, "Arrival")
            .genres(names(&["Drama"]))
            .media_file_id(Some(arrival_file))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let sicario_file = add_media_file(&db_context, Some(6_000_000)).await;
        movie(&db_context, "Sicario")
            .genres(names(&["Drama"]))
            .media_file_id(Some(sicario_file))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let (noor, ravi, mina) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        stop_at(
            &db_context,
            noor,
            arrival_file,
            SignedDuration::from_mins(20),
            "2026-11-02T20:00:00Z",
        )
        .await;
        stop_at(
            &db_context,
            noor,
            sicario_file,
            SignedDuration::from_mins(25),
            "2026-11-02T21:00:00Z",
        )
        .await;
        watch_to_the_end(&db_context, ravi, arrival_file, "2026-11-02T20:00:00Z").await;

        let mut views = Vec::new();
        for user_id in [noor, ravi, mina] {
            let Output::Movie(page) = call(dependencies(&db_context), user_id, arrival.id)
                .await
                .unwrap()
            else {
                panic!("the movie is not found");
            };
            let sicario = &page.more_like_this[0];
            views.push((
                page.resume_position_seconds,
                page.progress,
                page.minutes_left,
                page.watched,
                sicario.progress,
            ));
        }

        assert_eq!(
            views,
            [
                (Some(1200), Some(0.5), Some(20), false, Some(0.25)),
                (None, None, None, true, None),
                (None, None, None, false, None),
            ]
        );
    }

    #[tokio::test]
    async fn a_movie_without_a_clearlogo_has_no_logo() {
        let mut db_context = mock_db_context().await;
        let arrival = movie(&db_context, "Arrival")
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        add_artwork(&mut db_context, arrival.id, ArtworkKind::Fanart).await;

        let Output::Movie(page) = get(&db_context, arrival.id).await else {
            panic!("the movie is not found");
        };

        assert!(page.images.fanart.is_some());
        assert!(page.images.logo.is_none());
    }

    #[tokio::test]
    async fn an_unknown_movie_or_one_not_in_the_library_is_not_found() {
        let mut db_context = mock_db_context().await;
        let out_of_library = movie(&db_context, "Arrival")
            .in_library(false)
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        let out_of_library = get(&db_context, out_of_library.id).await;
        let unknown = get(&db_context, Uuid::new_v4()).await;

        assert!(
            matches!(out_of_library, Output::NotFound),
            "{out_of_library:?}"
        );
        assert!(matches!(unknown, Output::NotFound), "{unknown:?}");
    }
}
