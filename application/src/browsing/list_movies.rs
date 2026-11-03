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
use crate::ports::persistence::DbScope;
use domain::GridSort;
use domain::Movie;
use domain::sort_key;
use std::cmp::Reverse;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    genre: Option<String>,
    sort: GridSort,
    after: Option<Cursor>,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let movies = db_context
        .movies()
        .filter(Movie::fields().visible().eq(true))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .filter(|movie| genre.as_ref().is_none_or(|x| movie.genres.contains(x)))
        .map(|movie| {
            let key = match sort {
                GridSort::Title => {
                    SortKey::Title(sort_key(movie.sort_title.as_deref(), &movie.title).0)
                }
                GridSort::Added => SortKey::Added(Reverse(movie.added_at)),
                GridSort::Released => SortKey::Released(Reverse(movie.premiered)),
            };
            (Cursor { key, id: movie.id }, movie)
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
    use crate::test::movie;
    use jiff::Timestamp;
    use jiff::civil::date;

    async fn titles(
        db_context: &DbContext,
        genre: Option<&str>,
        sort: GridSort,
        after: Option<Cursor>,
    ) -> (Vec<String>, Option<Cursor>) {
        let output = call(
            Dependencies {
                db_context: db_context.clone(),
            },
            genre.map(str::to_owned),
            sort,
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
        for (title, sort_title, visible) in [
            ("Sicario", None, true),
            ("The Arrival", Some("Arrival"), true),
            ("dune", None, true),
            ("Hidden", None, false),
            ("Blade Runner 2049", None, true),
        ] {
            movie(&db_context, title)
                .sort_title(sort_title.map(str::to_owned))
                .visible(visible)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        let (titles, next) = titles(&db_context, None, GridSort::Title, None).await;

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
            movie(&db_context, &format!("Movie {number:02}"))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        let (first, next) = titles(&db_context, None, GridSort::Title, None).await;
        let (second, last) = titles(&db_context, None, GridSort::Title, next).await;

        assert_eq!(first.len(), 60);
        assert_eq!(second, ["Movie 60"]);
        assert_eq!(last, None);
    }

    #[tokio::test]
    async fn the_genre_filter_keeps_only_the_movies_of_that_genre() {
        let mut db_context = mock_db_context().await;
        for (title, genres) in [
            ("Arrival", &["Drama", "Science Fiction"][..]),
            ("Dune", &["Science Fiction"][..]),
            ("Sicario", &["Crime", "Drama"][..]),
        ] {
            movie(&db_context, title)
                .genres(genres.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>())
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        let (titles, _) = titles(&db_context, Some("Drama"), GridSort::Title, None).await;

        assert_eq!(titles, ["Arrival", "Sicario"]);
    }

    #[tokio::test]
    async fn the_added_sort_lists_the_newest_added_first_and_undated_last() {
        let mut db_context = mock_db_context().await;
        for (title, added_at) in [
            ("Arrival", Some("2026-10-29T08:00:00Z")),
            ("Blade Runner 2049", None),
            ("Dune", Some("2026-10-20T08:00:00Z")),
            ("Sicario", Some("2026-10-22T08:00:00Z")),
        ] {
            movie(&db_context, title)
                .added_at(added_at.map(|x| x.parse::<Timestamp>().unwrap()))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        let (titles, _) = titles(&db_context, None, GridSort::Added, None).await;

        assert_eq!(titles, ["Arrival", "Sicario", "Dune", "Blade Runner 2049"]);
    }

    #[tokio::test]
    async fn the_released_sort_lists_the_newest_premiere_first_and_undated_last() {
        let mut db_context = mock_db_context().await;
        for (title, premiered) in [
            ("Arrival", Some(date(2016, 11, 11))),
            ("Blade Runner 2049", Some(date(2017, 10, 6))),
            ("Dune", None),
            ("Sicario", Some(date(2015, 9, 18))),
        ] {
            movie(&db_context, title)
                .premiered(premiered)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        let (titles, _) = titles(&db_context, None, GridSort::Released, None).await;

        assert_eq!(titles, ["Blade Runner 2049", "Arrival", "Sicario", "Dune"]);
    }
}
