use crate::DbContext;
use domain::ArtworkKind;
use jiff::Timestamp;
use jiff::civil::Date;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to list the movies")]
pub struct Error;

#[derive(Debug)]
pub struct Movie {
    pub id: Uuid,
    pub title: String,
    pub sort_title: Option<String>,
    pub original_title: Option<String>,
    pub year: i64,
    pub overview: Option<String>,
    pub runtime_minutes: Option<i64>,
    pub certification: Option<String>,
    pub premiered: Option<Date>,
    pub studio: Option<String>,
    pub genres: Vec<String>,
    pub rating: Option<f64>,
    pub directors: Vec<String>,
    pub actors: Vec<Actor>,
    pub artwork: Vec<Artwork>,
    pub added_at: Option<Timestamp>,
    pub media_file_id: Option<Uuid>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Actor {
    pub name: String,
    pub role: Option<String>,
    pub order: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Artwork {
    pub kind: ArtworkKind,
    pub path: String,
    pub modified_at: Timestamp,
}

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Vec<Movie>, Error> {
    let mut db_context = dependencies.into().db_context;
    let mut movies = domain::Movie::filter(domain::Movie::fields().visible().eq(true))
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
    movies.sort_by_cached_key(|x| sort_key(x.sort_title.as_ref(), &x.title));
    Ok(movies)
}

pub(crate) fn sort_key(sort_title: Option<&String>, title: &str) -> (String, String) {
    (
        sort_title.map_or(title, String::as_str).to_lowercase(),
        title.to_owned(),
    )
}

pub(crate) fn actors(stored: Vec<domain::Actor>) -> Vec<Actor> {
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

pub(crate) fn artwork(stored: Vec<domain::Artwork>) -> Vec<Artwork> {
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
