use crate::DbContext;
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
    .exec(db_context.executor_mut())
    .await
    .map_err(Error::caused_by)?
    .into_iter()
    .map(|x| Episode {
        id: x.id,
        number: x.number,
        title: x.title,
        air_date: x.air_date,
        overview: x.overview,
        added_at: x.added_at,
        media_file_id: x.media_file_id,
    })
    .collect())
}
