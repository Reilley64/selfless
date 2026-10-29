use crate::DbContext;
use crate::ports::GetRadarrMovie;
use domain::Movie;
use tracing::Instrument;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub get_radarr_movie: GetRadarrMovie,
}

#[ohno::error]
#[display("failed to sync a movie")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>, radarr_id: i64) -> Result<(), Error> {
    let Dependencies {
        mut db_context,
        get_radarr_movie,
    } = dependencies.into();
    async {
        let Some(radarr_movie) = get_radarr_movie
            .call(radarr_id)
            .await
            .map_err(Error::caused_by)?
        else {
            Movie::filter_by_radarr_id(radarr_id)
                .delete()
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?;
            return Ok(());
        };
        let visible = match &radarr_movie.movie_file {
            Some(file) => matches!(tokio::fs::try_exists(&file.path).await, Ok(true)),
            None => false,
        };
        let date_added = radarr_movie.movie_file.map(|x| x.date_added);
        match Movie::filter_by_tmdb_id(radarr_movie.tmdb_id)
            .first()
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
        {
            Some(mut movie) => {
                let added_at = movie.added_at.or(date_added);
                movie
                    .update()
                    .radarr_id(radarr_id)
                    .title(radarr_movie.title)
                    .year(radarr_movie.year)
                    .overview(radarr_movie.overview)
                    .genres(radarr_movie.genres)
                    .added_at(added_at)
                    .visible(visible)
                    .exec(db_context.executor_mut())
                    .await
                    .map_err(Error::caused_by)?;
            }
            None => {
                toasty::create!(Movie {
                    tmdb_id: radarr_movie.tmdb_id,
                    radarr_id,
                    title: radarr_movie.title,
                    year: radarr_movie.year,
                    overview: radarr_movie.overview,
                    genres: radarr_movie.genres,
                    added_at: date_added,
                    visible,
                })
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?;
            }
        }
        Ok(())
    }
    .instrument(tracing::info_span!(
        "sync_movie",
        otel.name = "sync radarr movie"
    ))
    .await
}
