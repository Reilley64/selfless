use crate::library::Prober;
use crate::library::library_items;
use crate::library::library_items::Change;
use crate::library::media_files;
use crate::library::media_files::Inspected;
use crate::library::metadata;
use crate::library::metadata::FoundArtwork;
use crate::library::sync::Error;
use crate::ports::FindMediaFile;
use crate::ports::ListSubtitleFiles;
use crate::ports::ReadMovieFolder;
use crate::ports::ReadMovieNfo;
use crate::ports::SourceMovie;
use crate::ports::UnreadableFile;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use domain::MediaFile;
use domain::Movie;
use domain::MovieMetadata;
use domain::MovieNfo;
use domain::Nfo;
use domain::Owner;
use domain::Source;
use jiff::Timestamp;
use std::collections::HashSet;
use tracing::Span;
use uuid::Uuid;

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

pub(crate) struct MovieOnDisk {
    media_file: Option<Inspected>,
    artwork: Vec<FoundArtwork>,
    nfo: Nfo<MovieNfo>,
}

pub(crate) enum InspectedMovie {
    Read(Box<MovieOnDisk>),
    Unreadable(UnreadableFile),
}

impl InspectedMovie {
    pub(crate) fn unreadable(&self) -> bool {
        matches!(self, InspectedMovie::Unreadable(_))
    }
}

pub(crate) async fn inspect(
    files: &MovieFiles,
    stored_file: Option<&MediaFile>,
    stored_nfo_modified_at: Option<Timestamp>,
    source_movie: &SourceMovie,
) -> Result<InspectedMovie, Error> {
    let Some(file) = &source_movie.media_file else {
        return Ok(InspectedMovie::Read(Box::new(MovieOnDisk {
            media_file: None,
            artwork: Vec::new(),
            nfo: Nfo::Missing,
        })));
    };
    let read = async {
        let found = files.find_media_file.call(&file.path).await?;
        let mut folder = files.read_movie_folder.call(&file.path).await?;
        let nfo = metadata::read_nfo(folder.nfo.take(), stored_nfo_modified_at, |path| {
            files.read_movie_nfo.call(path)
        })
        .await?;
        Ok::<_, UnreadableFile>((found, folder, nfo))
    };
    let (found, folder, nfo) = match read.await {
        Ok(read) => read,
        Err(error) => {
            metadata::warn_unreadable_file(&error);
            return Ok(InspectedMovie::Unreadable(error));
        }
    };
    let media_file = media_files::inspect(
        &files.list_subtitle_files,
        &files.prober,
        stored_file,
        &file.path,
        found,
    )
    .await
    .map_err(Error::caused_by)?;
    Ok(InspectedMovie::Read(Box::new(MovieOnDisk {
        media_file: Some(media_file),
        artwork: metadata::item_artwork(folder.poster, folder.fanart, folder.clearlogo),
        nfo,
    })))
}

struct SyncedMovie {
    radarr_id: i64,
    metadata: MovieMetadata,
    nfo_modified_at: Option<Timestamp>,
    added_at: Option<Timestamp>,
    media_file_id: Option<Uuid>,
    visible: bool,
}

macro_rules! set_movie {
    ($builder:expr, $synced:expr) => {{
        let SyncedMovie {
            radarr_id,
            metadata,
            nfo_modified_at,
            added_at,
            media_file_id,
            visible,
        } = $synced;
        $builder
            .radarr_id(radarr_id)
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
    }};
}

pub(crate) async fn save(
    db_context: &mut DbContext,
    mut source_movie: SourceMovie,
    inspected: InspectedMovie,
) -> Result<(), Error> {
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let stored = transaction
        .movies()
        .filter(Movie::fields().tmdb_id().eq(source_movie.tmdb_id))
        .first()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let (media_file, artwork, nfo, unreadable) = match inspected {
        InspectedMovie::Read(on_disk) => {
            let MovieOnDisk {
                media_file,
                artwork,
                nfo,
            } = *on_disk;
            (media_file, artwork, nfo, None)
        }
        InspectedMovie::Unreadable(error) => (None, Vec::new(), Nfo::Missing, Some(error.path)),
    };
    if unreadable.is_some() {
        if let Some(movie) = &stored {
            metadata::save_unreadable_file(&mut transaction, Owner::Movie(movie.id), unreadable)
                .await
                .map_err(Error::caused_by)?;
            transaction.commit().await.map_err(Error::caused_by)?;
            return Ok(());
        }
        source_movie.media_file = None;
    }
    let visible = Movie::in_library(
        source_movie.media_file.is_some(),
        media_file.as_ref().is_some_and(Inspected::on_disk),
    );
    let source_added_at = source_movie.media_file.as_ref().map(|x| x.added_at);
    let (media_file_id, media_file_changed) = match (&source_movie.media_file, media_file) {
        (Some(file), Some(inspected)) => {
            let saved = media_files::save(&mut transaction, &file.path, inspected)
                .await
                .map_err(Error::caused_by)?;
            (Some(saved.id), saved.changed)
        }
        _ => (None, false),
    };
    let source_metadata = MovieMetadata::from_source(
        source_movie.title,
        source_movie.year,
        source_movie.overview,
        source_movie.genres,
    );
    let (metadata, nfo_modified_at) = nfo.metadata(
        stored.as_ref().map(|x| (x.metadata(), x.nfo_modified_at)),
        source_metadata,
        MovieMetadata::with_nfo,
    );
    let had_nfo = stored.as_ref().is_some_and(|x| x.nfo_modified_at.is_some());
    let synced = SyncedMovie {
        radarr_id: source_movie.radarr_id,
        metadata,
        nfo_modified_at,
        added_at: domain::first_added_at(stored.as_ref().and_then(|x| x.added_at), source_added_at),
        media_file_id,
        visible,
    };
    let (movie_id, change) = match stored {
        Some(mut movie) => {
            let old_media_file_id = movie.media_file_id;
            let changed = media_file_changed
                || movie.radarr_id != synced.radarr_id
                || movie.added_at != synced.added_at
                || movie.media_file_id != synced.media_file_id
                || movie.visible != synced.visible
                || movie.nfo_modified_at != synced.nfo_modified_at
                || movie.metadata() != synced.metadata;
            if changed {
                set_movie!(movie.update(), synced)
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
            let movie = set_movie!(
                transaction.movies().create().tmdb_id(source_movie.tmdb_id),
                synced
            )
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
    metadata::save_unreadable_file(&mut transaction, owner, unreadable)
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
