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
