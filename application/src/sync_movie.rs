use crate::DbContext;
use crate::library_items;
use crate::library_items::Change;
use crate::media_files;
use crate::ports::GetRadarrMovie;
use crate::ports::RadarrMovie;
use domain::MediaFile;
use domain::Movie;
use domain::Source;
use tracing::Instrument;
use tracing::Span;

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
        match get_radarr_movie
            .call(radarr_id)
            .await
            .map_err(Error::caused_by)?
        {
            Some(radarr_movie) => save(&mut db_context, radarr_movie).await,
            None => {
                let movies = Movie::filter_by_radarr_id(radarr_id)
                    .exec(db_context.executor_mut())
                    .await
                    .map_err(Error::caused_by)?;
                remove(&mut db_context, movies).await
            }
        }
    }
    .instrument(span())
    .await
}

pub(crate) fn span() -> Span {
    tracing::info_span!("sync_movie", otel.name = "sync radarr movie")
}

pub(crate) async fn save(
    db_context: &mut DbContext,
    radarr_movie: RadarrMovie,
) -> Result<(), Error> {
    let on_disk = match &radarr_movie.movie_file {
        Some(file) => media_files::on_disk(&file.path).await,
        None => None,
    };
    let visible = on_disk.is_some();
    let date_added = radarr_movie.movie_file.as_ref().map(|x| x.date_added);
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let db = transaction.executor_mut();
    let (media_file_id, media_file_changed) = match &radarr_movie.movie_file {
        Some(file) => {
            let (id, changed) = media_files::save(db, &file.path, on_disk)
                .await
                .map_err(Error::caused_by)?;
            (Some(id), changed)
        }
        None => (None, false),
    };
    let change = match Movie::filter_by_tmdb_id(radarr_movie.tmdb_id)
        .first()
        .exec(db)
        .await
        .map_err(Error::caused_by)?
    {
        Some(mut movie) => {
            let added_at = movie.added_at.or(date_added);
            let old_media_file_id = movie.media_file_id;
            let changed = media_file_changed
                || movie.radarr_id != radarr_movie.radarr_id
                || movie.title != radarr_movie.title
                || movie.year != radarr_movie.year
                || movie.overview != radarr_movie.overview
                || movie.genres != radarr_movie.genres
                || movie.added_at != added_at
                || movie.media_file_id != media_file_id
                || movie.visible != visible;
            if changed {
                movie
                    .update()
                    .radarr_id(radarr_movie.radarr_id)
                    .title(radarr_movie.title)
                    .year(radarr_movie.year)
                    .overview(radarr_movie.overview)
                    .genres(radarr_movie.genres)
                    .added_at(added_at)
                    .media_file_id(media_file_id)
                    .visible(visible)
                    .exec(db)
                    .await
                    .map_err(Error::caused_by)?;
            }
            if let Some(old_media_file_id) = old_media_file_id
                && Some(old_media_file_id) != media_file_id
            {
                MediaFile::filter_by_id(old_media_file_id)
                    .delete()
                    .exec(db)
                    .await
                    .map_err(Error::caused_by)?;
            }
            changed.then_some(Change::Changed)
        }
        None => {
            toasty::create!(Movie {
                tmdb_id: radarr_movie.tmdb_id,
                radarr_id: radarr_movie.radarr_id,
                title: radarr_movie.title,
                year: radarr_movie.year,
                overview: radarr_movie.overview,
                genres: radarr_movie.genres,
                added_at: date_added,
                media_file_id,
                visible,
            })
            .exec(db)
            .await
            .map_err(Error::caused_by)?;
            Some(Change::Added)
        }
    };
    transaction.commit().await.map_err(Error::caused_by)?;
    if let Some(change) = change {
        library_items::count(Source::Radarr, change);
    }
    Ok(())
}

pub(crate) async fn remove(db_context: &mut DbContext, movies: Vec<Movie>) -> Result<(), Error> {
    for movie in movies {
        let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
        let db = transaction.executor_mut();
        let media_file_id = movie.media_file_id;
        movie.delete().exec(db).await.map_err(Error::caused_by)?;
        if let Some(media_file_id) = media_file_id {
            MediaFile::filter_by_id(media_file_id)
                .delete()
                .exec(db)
                .await
                .map_err(Error::caused_by)?;
        }
        transaction.commit().await.map_err(Error::caused_by)?;
        library_items::count(Source::Radarr, Change::Removed);
    }
    Ok(())
}
