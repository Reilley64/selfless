use crate::DbContext;
use jiff::Timestamp;
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
    pub year: i64,
    pub overview: Option<String>,
    pub genres: Vec<String>,
    pub added_at: Option<Timestamp>,
}

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Vec<Movie>, Error> {
    let mut db_context = dependencies.into().db_context;
    Ok(
        domain::Movie::filter(domain::Movie::fields().visible().eq(true))
            .order_by(domain::Movie::fields().title().asc())
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| Movie {
                id: x.id,
                title: x.title,
                year: x.year,
                overview: x.overview,
                genres: x.genres,
                added_at: x.added_at,
            })
            .collect(),
    )
}
