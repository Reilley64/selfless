mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Actor;
pub use output::Artwork;
pub use output::Movie;
pub use output::Output;

use crate::ports::persistence::DbScope;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let mut movies = db_context
        .movies()
        .filter(domain::Movie::in_library_filter())
        .include(domain::Movie::fields().actors())
        .include(domain::Movie::fields().artwork())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|movie| Movie {
            id: movie.id,
            title: movie.title,
            sort_title: movie.sort_title,
            original_title: movie.original_title,
            year: movie.year,
            overview: movie.overview,
            runtime_minutes: movie.runtime_minutes,
            certification: movie.certification,
            premiered: movie.premiered,
            studio: movie.studio,
            genres: movie.genres,
            rating: movie.rating,
            directors: movie.directors,
            actors: actors(movie.actors.into_inner()),
            artwork: artwork(movie.artwork.into_inner()),
            added_at: movie.added_at,
            media_file_id: movie.media_file_id,
        })
        .collect::<Vec<_>>();
    movies.sort_by_cached_key(|x| domain::sort_key(x.sort_title.as_deref(), &x.title));
    Ok(Output { movies })
}

fn actors(stored: Vec<domain::Actor>) -> Vec<Actor> {
    let mut actors = stored
        .into_iter()
        .map(|x| Actor {
            name: x.name,
            role: x.role,
            order: x.order,
        })
        .collect::<Vec<_>>();
    actors.sort_by_key(|x| x.order);
    actors
}

fn artwork(stored: Vec<domain::Artwork>) -> Vec<Artwork> {
    let mut artwork = stored
        .into_iter()
        .map(|x| Artwork {
            kind: x.kind,
            path: x.path,
            modified_at: x.modified_at,
        })
        .collect::<Vec<_>>();
    artwork.sort_by(|a, b| a.path.cmp(&b.path));
    artwork
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::persistence::DbContext;
    use crate::test::at;
    use crate::test::mock_db_context;
    use domain::ArtworkKind;
    use uuid::Uuid;

    async fn seed_movie(
        db_context: &mut DbContext,
        tmdb_id: i64,
        title: &str,
        visible: bool,
    ) -> Uuid {
        db_context
            .movies()
            .create()
            .tmdb_id(tmdb_id)
            .radarr_id(tmdb_id)
            .title(title)
            .year(2016)
            .genres(Vec::<String>::new())
            .directors(Vec::<String>::new())
            .visible(visible)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    #[tokio::test]
    async fn only_movies_in_the_library_are_listed() {
        let mut db_context = mock_db_context().await;
        seed_movie(&mut db_context, 1, "Arrival", true).await;
        seed_movie(&mut db_context, 2, "Sicario", false).await;

        let movies = call(Dependencies { db_context }).await.unwrap().movies;

        assert_eq!(
            movies.iter().map(|x| x.title.as_str()).collect::<Vec<_>>(),
            ["Arrival"]
        );
    }

    #[tokio::test]
    async fn a_movies_artwork_is_listed_by_path() {
        let mut db_context = mock_db_context().await;
        let id = seed_movie(&mut db_context, 1, "Arrival", true).await;
        for (kind, path) in [
            (
                ArtworkKind::Poster,
                "/media/Movies/Arrival (2016)/poster.jpg",
            ),
            (
                ArtworkKind::Clearlogo,
                "/media/Movies/Arrival (2016)/clearlogo.png",
            ),
            (
                ArtworkKind::Fanart,
                "/media/Movies/Arrival (2016)/fanart.jpg",
            ),
        ] {
            db_context
                .artwork()
                .create()
                .movie_id(Some(id))
                .kind(kind)
                .path(path)
                .modified_at(at("2026-10-28T12:30:00Z"))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        let [movie] = call(Dependencies { db_context })
            .await
            .unwrap()
            .movies
            .try_into()
            .unwrap();

        assert_eq!(
            movie.artwork,
            [
                (
                    ArtworkKind::Clearlogo,
                    "/media/Movies/Arrival (2016)/clearlogo.png"
                ),
                (
                    ArtworkKind::Fanart,
                    "/media/Movies/Arrival (2016)/fanart.jpg"
                ),
                (
                    ArtworkKind::Poster,
                    "/media/Movies/Arrival (2016)/poster.jpg"
                ),
            ]
            .map(|(kind, path)| Artwork {
                kind,
                path: path.to_owned(),
                modified_at: at("2026-10-28T12:30:00Z"),
            })
        );
    }
}
