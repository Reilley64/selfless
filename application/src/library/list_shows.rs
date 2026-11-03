mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Actor;
pub use output::Artwork;
pub use output::Output;
pub use output::Show;

use crate::ports::persistence::DbScope;
use domain::Episode;
use domain::Season;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
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
    shows.sort_by_cached_key(|x| domain::sort_key(x.sort_title.as_deref(), &x.title));
    Ok(Output { shows })
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
