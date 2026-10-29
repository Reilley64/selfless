use crate::DbContext;
use crate::list_movies::Artwork;
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
    let episodes = domain::Episode::filter(
        domain::Episode::fields()
            .season_id()
            .eq(season_id)
            .and(domain::Episode::fields().visible().eq(true)),
    )
    .order_by(domain::Episode::fields().number().asc())
    .exec(db_context.executor_mut())
    .await
    .map_err(Error::caused_by)?;
    let mut listed = Vec::with_capacity(episodes.len());
    for episode in episodes {
        let artwork = domain::Artwork::filter_by_episode_id(episode.id)
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| Artwork {
                kind: x.kind,
                path: x.path,
                modified_at: x.modified_at,
            })
            .collect();
        listed.push(Episode {
            id: episode.id,
            number: episode.number,
            title: episode.title,
            air_date: episode.air_date,
            overview: episode.overview,
            added_at: episode.added_at,
            media_file_id: episode.media_file_id,
            rating: episode.rating,
            artwork,
        });
    }
    Ok(listed)
}
