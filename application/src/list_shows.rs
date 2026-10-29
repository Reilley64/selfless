use crate::DbContext;
use crate::list_movies::Actor;
use crate::list_movies::Artwork;
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
    let shows = domain::Show::filter(
        domain::Show::fields().seasons().any(
            Season::fields()
                .episodes()
                .any(Episode::fields().visible().eq(true)),
        ),
    )
    .order_by(domain::Show::fields().title().asc())
    .exec(db_context.executor_mut())
    .await
    .map_err(Error::caused_by)?;
    let mut listed = Vec::with_capacity(shows.len());
    for show in shows {
        let mut actors = domain::Actor::filter_by_show_id(show.id)
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| Actor {
                name: x.name,
                role: x.role,
                order: x.order,
            })
            .collect::<Vec<_>>();
        actors.sort_by_key(|x| x.order);
        let mut artwork = domain::Artwork::filter(
            domain::Artwork::fields()
                .show_id()
                .eq(show.id)
                .and(domain::Artwork::fields().season_id().is_none())
                .and(domain::Artwork::fields().episode_id().is_none()),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| Artwork {
            kind: x.kind,
            path: x.path,
            modified_at: x.modified_at,
        })
        .collect::<Vec<_>>();
        artwork.sort_by(|a, b| a.path.cmp(&b.path));
        listed.push(Show {
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
            actors,
            artwork,
        });
    }
    Ok(listed)
}
