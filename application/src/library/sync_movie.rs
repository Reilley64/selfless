mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::library::Prober;
use crate::library::library_items;
use crate::library::library_items::Change;
use crate::library::media_files;
use crate::library::media_files::Inspected;
use crate::library::metadata;
use crate::library::metadata::Nfo;
use crate::library::metadata::Owner;
use crate::ports::FindMediaFile;
use crate::ports::ListSubtitleFiles;
use crate::ports::MovieFolder;
use crate::ports::RadarrMovie;
use crate::ports::ReadMovieFolder;
use crate::ports::ReadMovieNfo;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use domain::MediaFile;
use domain::Movie;
use domain::MovieMetadata;
use domain::Source;
use std::collections::HashSet;
use tracing::Instrument;
use tracing::Span;

pub async fn call(dependencies: impl Into<Dependencies>, radarr_id: i64) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        find_media_file,
        get_radarr_movie,
        list_subtitle_files,
        prober,
        read_movie_folder,
        read_movie_nfo,
        source_health,
    } = dependencies.into();
    let files = MovieFiles {
        find_media_file,
        list_subtitle_files,
        prober,
        read_movie_folder,
        read_movie_nfo,
    };
    async {
        let radarr_movie = get_radarr_movie.call(radarr_id).await;
        match source_health
            .api_call(Source::Radarr, radarr_movie)
            .map_err(Error::caused_by)?
        {
            Some(radarr_movie) => {
                let stored = match &radarr_movie.movie_file {
                    Some(file) => db_context
                        .media_files()
                        .filter(MediaFile::fields().path().eq(&file.path))
                        .first()
                        .exec(db_context.executor_mut())
                        .await
                        .map_err(Error::caused_by)?,
                    None => None,
                };
                let inspected = inspect(&files, stored.as_ref(), &radarr_movie).await?;
                save(&mut db_context, &files, radarr_movie, inspected).await
            }
            None => {
                let movies = db_context
                    .movies()
                    .filter(Movie::fields().radarr_id().eq(radarr_id))
                    .exec(db_context.executor_mut())
                    .await
                    .map_err(Error::caused_by)?;
                remove(&mut db_context, movies).await
            }
        }
    }
    .instrument(span())
    .await?;
    Ok(Output)
}

pub(crate) struct MovieFiles {
    pub(crate) find_media_file: FindMediaFile,
    pub(crate) list_subtitle_files: ListSubtitleFiles,
    pub(crate) prober: Prober,
    pub(crate) read_movie_folder: ReadMovieFolder,
    pub(crate) read_movie_nfo: ReadMovieNfo,
}

pub(crate) fn span() -> Span {
    tracing::info_span!("sync_movie", otel.name = "sync radarr movie")
}

pub(crate) async fn inspect(
    files: &MovieFiles,
    stored: Option<&MediaFile>,
    radarr_movie: &RadarrMovie,
) -> Result<Option<Inspected>, Error> {
    match &radarr_movie.movie_file {
        Some(file) => Ok(Some(
            media_files::inspect(
                &files.find_media_file,
                &files.list_subtitle_files,
                &files.prober,
                stored,
                &file.path,
            )
            .await
            .map_err(Error::caused_by)?,
        )),
        None => Ok(None),
    }
}

pub(crate) async fn save(
    db_context: &mut DbContext,
    files: &MovieFiles,
    radarr_movie: RadarrMovie,
    inspected: Option<Inspected>,
) -> Result<(), Error> {
    let folder = match &radarr_movie.movie_file {
        Some(file) => files.read_movie_folder.call(&file.path).await,
        None => MovieFolder::default(),
    };
    let visible = Movie::in_library(
        radarr_movie.movie_file.is_some(),
        inspected.as_ref().is_some_and(Inspected::on_disk),
    );
    let date_added = radarr_movie.movie_file.as_ref().map(|x| x.date_added);
    let artwork = metadata::item_artwork(folder.poster, folder.fanart, folder.clearlogo);
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let (media_file_id, media_file_changed) = match (&radarr_movie.movie_file, inspected) {
        (Some(file), Some(inspected)) => {
            let (id, changed) = media_files::save(&mut transaction, &file.path, inspected)
                .await
                .map_err(Error::caused_by)?;
            (Some(id), changed)
        }
        _ => (None, false),
    };
    let stored = transaction
        .movies()
        .filter(Movie::fields().tmdb_id().eq(radarr_movie.tmdb_id))
        .first()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let nfo = Nfo::read(
        folder.nfo,
        stored.as_ref().and_then(|x| x.nfo_modified_at),
        |path| files.read_movie_nfo.call(path),
    )
    .await;
    let radarr_metadata = MovieMetadata::from_radarr(
        radarr_movie.title,
        radarr_movie.year,
        radarr_movie.overview,
        radarr_movie.genres,
    );
    let (metadata, nfo_modified_at) = match (&nfo, &stored) {
        (Nfo::Unchanged, Some(movie)) => (movie.metadata(), movie.nfo_modified_at),
        (Nfo::Read(nfo, modified_at), _) => (radarr_metadata.with_nfo(nfo), Some(*modified_at)),
        _ => (radarr_metadata, None),
    };
    let had_nfo = stored.as_ref().is_some_and(|x| x.nfo_modified_at.is_some());
    let (movie_id, change) = match stored {
        Some(mut movie) => {
            let added_at = domain::first_added_at(movie.added_at, date_added);
            let old_media_file_id = movie.media_file_id;
            let changed = media_file_changed
                || movie.radarr_id != radarr_movie.radarr_id
                || movie.added_at != added_at
                || movie.media_file_id != media_file_id
                || movie.visible != visible
                || movie.nfo_modified_at != nfo_modified_at
                || movie.metadata() != metadata;
            if changed {
                movie
                    .update()
                    .radarr_id(radarr_movie.radarr_id)
                    .title(metadata.title)
                    .sort_title(metadata.sort_title)
                    .original_title(metadata.original_title)
                    .year(metadata.year)
                    .overview(metadata.overview)
                    .runtime_minutes(metadata.runtime_minutes)
                    .certification(metadata.certification)
                    .premiered(metadata.premiered)
                    .studio(metadata.studio)
                    .genres(metadata.genres)
                    .rating(metadata.rating)
                    .directors(metadata.directors)
                    .nfo_modified_at(nfo_modified_at)
                    .added_at(added_at)
                    .media_file_id(media_file_id)
                    .visible(visible)
                    .exec(transaction.executor_mut())
                    .await
                    .map_err(Error::caused_by)?;
            }
            if let Some(old_media_file_id) = old_media_file_id
                && Some(old_media_file_id) != media_file_id
            {
                media_files::delete(&mut transaction, vec![old_media_file_id])
                    .await
                    .map_err(Error::caused_by)?;
            }
            (movie.id, changed.then_some(Change::Changed))
        }
        None => {
            let movie = transaction
                .movies()
                .create()
                .tmdb_id(radarr_movie.tmdb_id)
                .radarr_id(radarr_movie.radarr_id)
                .title(metadata.title)
                .sort_title(metadata.sort_title)
                .original_title(metadata.original_title)
                .year(metadata.year)
                .overview(metadata.overview)
                .runtime_minutes(metadata.runtime_minutes)
                .certification(metadata.certification)
                .premiered(metadata.premiered)
                .studio(metadata.studio)
                .genres(metadata.genres)
                .rating(metadata.rating)
                .directors(metadata.directors)
                .nfo_modified_at(nfo_modified_at)
                .added_at(date_added)
                .media_file_id(media_file_id)
                .visible(visible)
                .exec(transaction.executor_mut())
                .await
                .map_err(Error::caused_by)?;
            (movie.id, Some(Change::Added))
        }
    };
    let owner = Owner::Movie(movie_id);
    metadata::save_actors(&mut transaction, owner, had_nfo, &nfo, |x| &x.actors)
        .await
        .map_err(Error::caused_by)?;
    let artwork_changed = metadata::save_artwork(&mut transaction, owner, artwork)
        .await
        .map_err(Error::caused_by)?;
    let unreadable_nfos = nfo
        .unreadable_path()
        .map(|path| (None, path.to_owned()))
        .into_iter()
        .collect::<HashSet<_>>();
    metadata::save_unreadable_nfos(&mut transaction, owner, Vec::new(), unreadable_nfos)
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    if let Some(change) = change.or(artwork_changed.then_some(Change::Changed)) {
        library_items::count(Source::Radarr, change);
    }
    Ok(())
}

pub(crate) async fn remove(db_context: &mut DbContext, movies: Vec<Movie>) -> Result<(), Error> {
    for movie in movies {
        let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
        let media_file_id = movie.media_file_id;
        metadata::delete(&mut transaction, Owner::Movie(movie.id))
            .await
            .map_err(Error::caused_by)?;
        movie
            .delete()
            .exec(transaction.executor_mut())
            .await
            .map_err(Error::caused_by)?;
        if let Some(media_file_id) = media_file_id {
            media_files::delete(&mut transaction, vec![media_file_id])
                .await
                .map_err(Error::caused_by)?;
        }
        transaction.commit().await.map_err(Error::caused_by)?;
        library_items::count(Source::Radarr, Change::Removed);
    }
    Ok(())
}
