use crate::DbContext;
use crate::list_movies::Artwork;
use crate::list_movies::artwork;
use jiff::Timestamp;
use jiff::civil::Date;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to list the episodes of a season")]
pub struct Error;

pub struct Episode {
    pub id: Uuid,
    pub number: i64,
    pub title: String,
    pub air_date: Option<Date>,
    pub overview: Option<String>,
    pub added_at: Option<Timestamp>,
    pub media_file_id: Option<Uuid>,
    pub rating: Option<f64>,
    pub artwork: Vec<Artwork>,
}

pub async fn call(
    dependencies: impl Into<Dependencies>,
    season_id: Uuid,
) -> Result<Vec<Episode>, Error> {
    let mut db_context = dependencies.into().db_context;
    Ok(domain::Episode::filter(
        domain::Episode::fields()
            .season_id()
            .eq(season_id)
            .and(domain::Episode::fields().visible().eq(true)),
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
    .collect())
}
