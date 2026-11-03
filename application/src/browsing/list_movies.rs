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
use domain::Movie;
use domain::sort_key;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    after: Option<Cursor>,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
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
    Ok(Output {
        tiles: movies
            .into_iter()
            .map(|movie| Tile {
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
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;
    use uuid::Uuid;

    async fn add_movie(
        db_context: &mut DbContext,
        title: &str,
        sort_title: Option<&str>,
        visible: bool,
    ) {
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
            .unwrap();
    }

    async fn titles(
        db_context: &DbContext,
        after: Option<Cursor>,
    ) -> (Vec<String>, Option<Cursor>) {
        let output = call(
            Dependencies {
                db_context: db_context.clone(),
            },
            after,
        )
        .await
        .unwrap();
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
}
