use crate::DbContext;
use crate::library_items;
use crate::library_items::Change;
use crate::media_files;
use crate::ports::FoundFile;
use crate::ports::GetRadarrMovie;
use crate::ports::MovieFolder;
use crate::ports::MovieNfo;
use crate::ports::RadarrMovie;
use crate::ports::ReadMovieFolder;
use crate::ports::ReadMovieNfo;
use domain::Actor;
use domain::Artwork;
use domain::ArtworkKind;
use domain::MediaFile;
use domain::Movie;
use domain::Source;
use domain::SyncProblem;
use domain::SyncProblemKind;
use jiff::Timestamp;
use jiff::civil::Date;
use tracing::Instrument;
use tracing::Span;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub get_radarr_movie: GetRadarrMovie,
    pub read_movie_folder: ReadMovieFolder,
    pub read_movie_nfo: ReadMovieNfo,
}

#[ohno::error]
#[display("failed to sync a movie")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>, radarr_id: i64) -> Result<(), Error> {
    let Dependencies {
        mut db_context,
        get_radarr_movie,
        read_movie_folder,
        read_movie_nfo,
    } = dependencies.into();
    async {
        match get_radarr_movie
            .call(radarr_id)
            .await
            .map_err(Error::caused_by)?
        {
            Some(radarr_movie) => {
                save(
                    &mut db_context,
                    &read_movie_folder,
                    &read_movie_nfo,
                    radarr_movie,
                )
                .await
            }
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

enum Nfo {
    Missing,
    Unchanged(Metadata),
    Read(MovieNfo, Timestamp),
    Unreadable(String),
}

#[derive(Clone, PartialEq)]
struct Metadata {
    title: String,
    sort_title: Option<String>,
    original_title: Option<String>,
    year: i64,
    overview: Option<String>,
    runtime_minutes: Option<i64>,
    certification: Option<String>,
    premiered: Option<Date>,
    studio: Option<String>,
    genres: Vec<String>,
    rating: Option<f64>,
    directors: Vec<String>,
    nfo_modified_at: Option<Timestamp>,
}

impl Metadata {
    fn stored(movie: &Movie) -> Self {
        Self {
            title: movie.title.clone(),
            sort_title: movie.sort_title.clone(),
            original_title: movie.original_title.clone(),
            year: movie.year,
            overview: movie.overview.clone(),
            runtime_minutes: movie.runtime_minutes,
            certification: movie.certification.clone(),
            premiered: movie.premiered,
            studio: movie.studio.clone(),
            genres: movie.genres.clone(),
            rating: movie.rating,
            directors: movie.directors.clone(),
            nfo_modified_at: movie.nfo_modified_at,
        }
    }

    fn of(nfo: &Nfo, radarr_movie: &RadarrMovie) -> Self {
        match nfo {
            Nfo::Unchanged(metadata) => metadata.clone(),
            Nfo::Read(nfo, modified_at) => Self {
                title: nfo
                    .title
                    .clone()
                    .unwrap_or_else(|| radarr_movie.title.clone()),
                sort_title: nfo.sort_title.clone(),
                original_title: nfo.original_title.clone(),
                year: nfo.year.unwrap_or(radarr_movie.year),
                overview: nfo.plot.clone().or_else(|| radarr_movie.overview.clone()),
                runtime_minutes: nfo.runtime_minutes,
                certification: nfo.certification.clone(),
                premiered: nfo.premiered,
                studio: nfo.studio.clone(),
                genres: if nfo.genres.is_empty() {
                    radarr_movie.genres.clone()
                } else {
                    nfo.genres.clone()
                },
                rating: nfo.rating,
                directors: nfo.directors.clone(),
                nfo_modified_at: Some(*modified_at),
            },
            Nfo::Missing | Nfo::Unreadable(_) => Self {
                title: radarr_movie.title.clone(),
                sort_title: None,
                original_title: None,
                year: radarr_movie.year,
                overview: radarr_movie.overview.clone(),
                runtime_minutes: None,
                certification: None,
                premiered: None,
                studio: None,
                genres: radarr_movie.genres.clone(),
                rating: None,
                directors: Vec::new(),
                nfo_modified_at: None,
            },
        }
    }
}

pub(crate) async fn save(
    db_context: &mut DbContext,
    read_movie_folder: &ReadMovieFolder,
    read_movie_nfo: &ReadMovieNfo,
    radarr_movie: RadarrMovie,
) -> Result<(), Error> {
    let (on_disk, folder) = match &radarr_movie.movie_file {
        Some(file) => (
            media_files::on_disk(&file.path).await,
            read_movie_folder.call(&file.path).await,
        ),
        None => (None, MovieFolder::default()),
    };
    let visible = on_disk.is_some();
    let date_added = radarr_movie.movie_file.as_ref().map(|x| x.date_added);
    let artwork = [
        (ArtworkKind::Poster, folder.poster),
        (ArtworkKind::Fanart, folder.fanart),
        (ArtworkKind::Clearlogo, folder.clearlogo),
    ]
    .into_iter()
    .filter_map(|(kind, file)| Some((kind, file?)))
    .collect::<Vec<_>>();
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
    let stored = Movie::filter_by_tmdb_id(radarr_movie.tmdb_id)
        .first()
        .exec(db)
        .await
        .map_err(Error::caused_by)?;
    let nfo = match folder.nfo {
        None => Nfo::Missing,
        Some(file) => match stored
            .as_ref()
            .filter(|x| x.nfo_modified_at == Some(file.modified_at))
        {
            Some(movie) => Nfo::Unchanged(Metadata::stored(movie)),
            None => match read_movie_nfo.call(&file.path).await {
                Ok(nfo) => Nfo::Read(nfo, file.modified_at),
                Err(error) => {
                    tracing::warn!(
                        error = &error as &dyn std::error::Error,
                        file.path = file.path,
                        "failed to read an NFO file"
                    );
                    Nfo::Unreadable(file.path)
                }
            },
        },
    };
    let metadata = Metadata::of(&nfo, &radarr_movie);
    let (movie_id, change) = match stored {
        Some(mut movie) => {
            let added_at = movie.added_at.or(date_added);
            let old_media_file_id = movie.media_file_id;
            let had_nfo = movie.nfo_modified_at.is_some();
            let changed = media_file_changed
                || movie.radarr_id != radarr_movie.radarr_id
                || movie.added_at != added_at
                || movie.media_file_id != media_file_id
                || movie.visible != visible
                || Metadata::stored(&movie) != metadata;
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
                    .nfo_modified_at(metadata.nfo_modified_at)
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
            if had_nfo && !matches!(nfo, Nfo::Unchanged(_)) {
                delete_actors(db, movie.id).await?;
            }
            (movie.id, changed.then_some(Change::Changed))
        }
        None => {
            let movie = toasty::create!(Movie {
                tmdb_id: radarr_movie.tmdb_id,
                radarr_id: radarr_movie.radarr_id,
                title: metadata.title,
                sort_title: metadata.sort_title,
                original_title: metadata.original_title,
                year: metadata.year,
                overview: metadata.overview,
                runtime_minutes: metadata.runtime_minutes,
                certification: metadata.certification,
                premiered: metadata.premiered,
                studio: metadata.studio,
                genres: metadata.genres,
                rating: metadata.rating,
                directors: metadata.directors,
                nfo_modified_at: metadata.nfo_modified_at,
                added_at: date_added,
                media_file_id,
                visible,
            })
            .exec(db)
            .await
            .map_err(Error::caused_by)?;
            (movie.id, Some(Change::Added))
        }
    };
    if let Nfo::Read(nfo, _) = &nfo {
        for actor in &nfo.actors {
            toasty::create!(Actor {
                movie_id,
                name: actor.name.clone(),
                role: actor.role.clone(),
                order: actor.order,
            })
            .exec(db)
            .await
            .map_err(Error::caused_by)?;
        }
    }
    let artwork_changed = save_artwork(db, movie_id, artwork).await?;
    let unreadable_nfo = match &nfo {
        Nfo::Unreadable(path) => Some(path.as_str()),
        _ => None,
    };
    save_unreadable_nfo(db, movie_id, unreadable_nfo).await?;
    transaction.commit().await.map_err(Error::caused_by)?;
    if let Some(change) = change.or(artwork_changed.then_some(Change::Changed)) {
        library_items::count(Source::Radarr, change);
    }
    Ok(())
}

async fn delete_actors(db: &mut dyn toasty::Executor, movie_id: Uuid) -> Result<(), Error> {
    Actor::filter_by_movie_id(movie_id)
        .delete()
        .exec(db)
        .await
        .map_err(Error::caused_by)
}

async fn save_artwork(
    db: &mut dyn toasty::Executor,
    movie_id: Uuid,
    found: Vec<(ArtworkKind, FoundFile)>,
) -> Result<bool, Error> {
    let stored = Artwork::filter_by_movie_id(movie_id)
        .exec(db)
        .await
        .map_err(Error::caused_by)?;
    if stored.len() == found.len()
        && found.iter().all(|(kind, file)| {
            stored.iter().any(|x| {
                x.kind == *kind && x.path == file.path && x.modified_at == file.modified_at
            })
        })
    {
        return Ok(false);
    }
    Artwork::filter_by_movie_id(movie_id)
        .delete()
        .exec(db)
        .await
        .map_err(Error::caused_by)?;
    for (kind, file) in found {
        toasty::create!(Artwork {
            movie_id,
            kind,
            path: file.path,
            modified_at: file.modified_at,
        })
        .exec(db)
        .await
        .map_err(Error::caused_by)?;
    }
    Ok(true)
}

async fn save_unreadable_nfo(
    db: &mut dyn toasty::Executor,
    movie_id: Uuid,
    path: Option<&str>,
) -> Result<(), Error> {
    let stored = SyncProblem::filter_by_movie_id(movie_id)
        .exec(db)
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .filter(|x| x.kind == SyncProblemKind::UnreadableNfo)
        .collect::<Vec<_>>();
    if stored.iter().map(|x| x.path.as_str()).eq(path) {
        return Ok(());
    }
    for problem in stored {
        problem.delete().exec(db).await.map_err(Error::caused_by)?;
    }
    if let Some(path) = path {
        toasty::create!(SyncProblem {
            movie_id,
            kind: SyncProblemKind::UnreadableNfo,
            path: path.to_owned(),
        })
        .exec(db)
        .await
        .map_err(Error::caused_by)?;
    }
    Ok(())
}

pub(crate) async fn remove(db_context: &mut DbContext, movies: Vec<Movie>) -> Result<(), Error> {
    for movie in movies {
        let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
        let db = transaction.executor_mut();
        let media_file_id = movie.media_file_id;
        delete_actors(db, movie.id).await?;
        Artwork::filter_by_movie_id(movie.id)
            .delete()
            .exec(db)
            .await
            .map_err(Error::caused_by)?;
        SyncProblem::filter_by_movie_id(movie.id)
            .delete()
            .exec(db)
            .await
            .map_err(Error::caused_by)?;
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
