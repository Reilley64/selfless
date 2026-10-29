use crate::DbContext;
use crate::list_movies::Actor;
use crate::list_movies::Artwork;
use crate::list_movies::actors;
use crate::list_movies::artwork;
use crate::list_movies::sort_key;
use domain::Episode;
use domain::Season;
use jiff::civil::Date;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to list the shows")]
pub struct Error;

#[derive(Debug)]
pub struct Show {
    pub id: Uuid,
    pub title: String,
    pub sort_title: Option<String>,
    pub year: Option<i64>,
    pub genres: Vec<String>,
    pub overview: Option<String>,
    pub certification: Option<String>,
    pub premiered: Option<Date>,
    pub end_date: Option<Date>,
    pub status: Option<String>,
    pub network: Option<String>,
    pub rating: Option<f64>,
    pub actors: Vec<Actor>,
    pub artwork: Vec<Artwork>,
}

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Vec<Show>, Error> {
    let mut db_context = dependencies.into().db_context;
    let mut shows = domain::Show::filter(
        domain::Show::fields().seasons().any(
            Season::fields()
                .episodes()
                .any(Episode::fields().visible().eq(true)),
        ),
    )
    .include(domain::Show::fields().actors())
    .include(
        domain::Show::fields().artwork().filter(
            domain::Artwork::fields()
                .season_id()
                .is_none()
                .and(domain::Artwork::fields().episode_id().is_none()),
        ),
    )
    .exec(db_context.executor_mut())
    .await
    .map_err(Error::caused_by)?
    .into_iter()
    .map(|show| Show {
        id: show.id,
        title: show.title,
        sort_title: show.sort_title,
        year: show.year,
        genres: show.genres,
        overview: show.overview,
        certification: show.certification,
        premiered: show.premiered,
        end_date: show.end_date,
        status: show.status,
        network: show.network,
        rating: show.rating,
        actors: actors(show.actors.into_inner()),
        artwork: artwork(show.artwork.into_inner()),
    })
    .collect::<Vec<_>>();
    shows.sort_by_cached_key(|x| sort_key(x.sort_title.as_ref(), &x.title));
    Ok(shows)
}
