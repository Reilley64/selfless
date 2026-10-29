use crate::DbContext;
use domain::Episode;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to list the seasons of a show")]
pub struct Error;

pub struct Season {
    pub id: Uuid,
    pub number: i64,
}

pub async fn call(
    dependencies: impl Into<Dependencies>,
    show_id: Uuid,
) -> Result<Vec<Season>, Error> {
    let mut db_context = dependencies.into().db_context;
    let mut seasons = domain::Season::filter(
        domain::Season::fields().show_id().eq(show_id).and(
            domain::Season::fields()
                .episodes()
                .any(Episode::fields().visible().eq(true)),
        ),
    )
    .exec(db_context.executor_mut())
    .await
    .map_err(Error::caused_by)?
    .into_iter()
    .map(|x| Season {
        id: x.id,
        number: x.number,
    })
    .collect::<Vec<_>>();
    seasons.sort_by_key(|x| (x.number == 0, x.number));
    Ok(seasons)
}
