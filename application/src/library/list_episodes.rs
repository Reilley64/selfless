mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Artwork;
pub use output::Episode;
pub use output::Output;

use crate::ports::persistence::DbScope;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, season_id: Uuid) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let episodes = db_context
        .episodes()
        .filter(
            domain::Episode::fields()
                .season_id()
                .eq(season_id)
                .and(domain::Episode::in_library_filter()),
        )
        .order_by(domain::Episode::fields().number().asc())
        .include(domain::Episode::fields().artwork())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|episode| Episode {
            id: episode.id,
            number: episode.number,
            title: episode.title,
            air_date: episode.air_date,
            overview: episode.overview,
            added_at: episode.added_at,
            media_file_id: episode.media_file_id,
            rating: episode.rating,
            artwork: artwork(episode.artwork.into_inner()),
        })
        .collect();
    Ok(Output { episodes })
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
