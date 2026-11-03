mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Artwork;
pub use output::Output;
pub use output::Season;

use crate::ports::persistence::DbScope;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, show_id: Uuid) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let mut seasons = db_context
        .seasons()
        .filter(
            domain::Season::fields()
                .show_id()
                .eq(show_id)
                .and(domain::Season::in_library_filter()),
        )
        .include(domain::Season::fields().artwork())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    seasons.sort_by_key(domain::Season::listing_order);
    let seasons = seasons
        .into_iter()
        .map(|x| Season {
            id: x.id,
            number: x.number,
            artwork: artwork(x.artwork.into_inner()),
        })
        .collect();
    Ok(Output { seasons })
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
