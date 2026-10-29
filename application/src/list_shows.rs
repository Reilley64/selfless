use crate::DbContext;
use domain::Episode;
use domain::Season;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to list the shows")]
pub struct Error;

pub struct Show {
    pub id: Uuid,
    pub title: String,
    pub year: Option<i64>,
    pub genres: Vec<String>,
    pub overview: Option<String>,
}

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Vec<Show>, Error> {
    let mut db_context = dependencies.into().db_context;
    Ok(domain::Show::filter(
        domain::Show::fields().seasons().any(
            Season::fields()
                .episodes()
                .any(Episode::fields().visible().eq(true)),
        ),
    )
    .order_by(domain::Show::fields().title().asc())
    .exec(db_context.executor_mut())
    .await
    .map_err(Error::caused_by)?
    .into_iter()
    .map(|x| Show {
        id: x.id,
        title: x.title,
        year: x.year,
        genres: x.genres,
        overview: x.overview,
    })
    .collect())
}
