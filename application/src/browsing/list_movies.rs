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
use domain::Movie;
use domain::sort_key;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    after: Option<Cursor>,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        url_signer,
    } = dependencies.into();
    let movies = Movie::filter(Movie::fields().visible().eq(true))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|movie| {
            let cursor = Cursor {
                sort_title: sort_key(movie.sort_title.as_deref(), &movie.title).0,
                id: movie.id,
            };
            (cursor, movie)
        })
        .collect();
    let (movies, next) = page(movies, after.as_ref());
    let mut images = title_images(
        &mut db_context,
        |x| x.movie_id,
        Artwork::fields()
            .movie_id()
            .in_list(movies.iter().map(|x| x.id).collect::<Vec<_>>()),
        &url_signer,
        clock.call(),
    )
    .await
    .map_err(Error::caused_by)?;
    Ok(Output {
        tiles: movies
            .into_iter()
            .map(|movie| Tile {
                images: images.remove(&movie.id).unwrap_or_default(),
                id: movie.id,
                title: movie.title,
                year: Some(movie.year),
                certification: movie.certification,
                genres: movie.genres,
                plot: movie.overview,
                kind: TileKind::Movie {
                    runtime_minutes: movie.runtime_minutes,
                },
            })
            .collect(),
        next,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browsing::Image;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;
    use crate::test::url_signer;
    use domain::ArtworkKind;
    use jiff::Timestamp;
    use test_fixtures::FakeClock;
    use uuid::Uuid;

    fn dependencies(db_context: &DbContext) -> Dependencies {
        Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(FakeClock::new().reader()),
            url_signer: url_signer(),
        }
    }

    async fn add_movie(
        db_context: &mut DbContext,
        title: &str,
        sort_title: Option<&str>,
        visible: bool,
    ) -> Movie {
        Movie::create()
            .tmdb_id(Uuid::new_v4().as_u64_pair().0 as i64)
            .radarr_id(1)
            .title(title)
            .year(2016)
            .genres(Vec::<String>::new())
            .directors(Vec::<String>::new())
            .sort_title(sort_title.map(str::to_owned))
            .visible(visible)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
    }

    async fn add_artwork(db_context: &mut DbContext, movie_id: Uuid, kind: ArtworkKind) -> Uuid {
        db_context
            .artwork()
            .create()
            .movie_id(Some(movie_id))
            .kind(kind)
            .path(format!("/mnt/media/movies/Arrival (2016)/{kind:?}"))
            .modified_at(Timestamp::UNIX_EPOCH)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    fn widths(image: Option<&Image>) -> Option<(Uuid, Vec<i64>)> {
        image.map(|x| (x.id, x.urls.iter().map(|x| x.width).collect()))
    }

    async fn titles(
        db_context: &DbContext,
        after: Option<Cursor>,
    ) -> (Vec<String>, Option<Cursor>) {
        let output = call(dependencies(db_context), after).await.unwrap();
        (
            output.tiles.into_iter().map(|x| x.title).collect(),
            output.next,
        )
    }

    #[tokio::test]
    async fn the_page_lists_every_visible_movie_by_sort_title() {
        let mut db_context = mock_db_context().await;
        add_movie(&mut db_context, "Sicario", None, true).await;
        add_movie(&mut db_context, "The Arrival", Some("Arrival"), true).await;
        add_movie(&mut db_context, "dune", None, true).await;
        add_movie(&mut db_context, "Hidden", None, false).await;
        add_movie(&mut db_context, "Blade Runner 2049", None, true).await;

        let (titles, next) = titles(&db_context, None).await;

        assert_eq!(
            titles,
            ["The Arrival", "Blade Runner 2049", "dune", "Sicario"]
        );
        assert_eq!(next, None);
    }

    #[tokio::test]
    async fn the_next_page_starts_after_its_cursor() {
        let mut db_context = mock_db_context().await;
        for number in 0..61 {
            add_movie(&mut db_context, &format!("Movie {number:02}"), None, true).await;
        }

        let (first, next) = titles(&db_context, None).await;
        let (second, last) = titles(&db_context, next).await;

        assert_eq!(first.len(), 60);
        assert_eq!(second, ["Movie 60"]);
        assert_eq!(last, None);
    }

    #[tokio::test]
    async fn a_movie_tile_carries_its_fanart_and_logo_at_every_width_they_come_in() {
        let mut db_context = mock_db_context().await;
        let arrival = add_movie(&mut db_context, "Arrival", None, true).await;
        let fanart = add_artwork(&mut db_context, arrival.id, ArtworkKind::Fanart).await;
        let logo = add_artwork(&mut db_context, arrival.id, ArtworkKind::Clearlogo).await;
        add_artwork(&mut db_context, arrival.id, ArtworkKind::Poster).await;
        add_movie(&mut db_context, "Dune", None, true).await;

        let output = call(dependencies(&db_context), None).await.unwrap();

        let [arrival, dune] = &output.tiles[..] else {
            panic!("two tiles: {:?}", output.tiles);
        };
        assert_eq!(
            widths(arrival.images.fanart.as_ref()),
            Some((fanart, vec![480, 960, 1280, 1920]))
        );
        assert_eq!(
            widths(arrival.images.logo.as_ref()),
            Some((logo, vec![300, 600]))
        );
        assert!(arrival.images.thumb.is_none());
        assert!(dune.images.fanart.is_none() && dune.images.logo.is_none());
    }
}
