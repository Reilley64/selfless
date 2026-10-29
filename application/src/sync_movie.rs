use crate::DbContext;
use crate::MediaFolder;
use crate::SourceHealth;
use crate::library_items;
use crate::library_items::Change;
use crate::media_files;
use crate::media_files::Inspected;
use crate::metadata;
use crate::metadata::FoundArtwork;
use crate::metadata::Nfo;
use crate::metadata::Owner;
use crate::ports::GetRadarrMovie;
use crate::ports::MovieFolder;
use crate::ports::MovieNfo;
use crate::ports::RadarrMovie;
use domain::ArtworkKind;
use domain::MediaFile;
use domain::Movie;
use domain::Source;
use jiff::Timestamp;
use jiff::civil::Date;
use std::collections::HashSet;
use tracing::Instrument;
use tracing::Span;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub get_radarr_movie: GetRadarrMovie,
    pub media_folder: MediaFolder,
    pub source_health: SourceHealth,
}

#[ohno::error]
#[display("failed to sync a movie")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>, radarr_id: i64) -> Result<(), Error> {
    let Dependencies {
        mut db_context,
        get_radarr_movie,
        media_folder,
        source_health,
    } = dependencies.into();
    async {
        let radarr_movie = get_radarr_movie.call(radarr_id).await;
        match source_health
            .api_call(Source::Radarr, radarr_movie)
            .map_err(Error::caused_by)?
        {
            Some(radarr_movie) => {
                let stored = match &radarr_movie.movie_file {
                    Some(file) => MediaFile::filter_by_path(&file.path)
                        .first()
                        .exec(db_context.executor_mut())
                        .await
                        .map_err(Error::caused_by)?,
                    None => None,
                };
                let inspected = inspect(&media_folder, stored.as_ref(), &radarr_movie).await?;
                save(&mut db_context, &media_folder, radarr_movie, inspected).await
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

    fn of(nfo: &Nfo<MovieNfo>, radarr_movie: &RadarrMovie, stored: Option<&Movie>) -> Self {
        match (nfo, stored) {
            (Nfo::Unchanged, Some(movie)) => Self::stored(movie),
            (Nfo::Read(nfo, modified_at), _) => Self {
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
            _ => Self {
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

pub(crate) async fn inspect(
    media_folder: &MediaFolder,
    stored: Option<&MediaFile>,
    radarr_movie: &RadarrMovie,
) -> Result<Option<Inspected>, Error> {
    match &radarr_movie.movie_file {
        Some(file) => Ok(Some(
            media_folder
                .inspect(stored, &file.path)
                .await
                .map_err(Error::caused_by)?,
        )),
        None => Ok(None),
    }
}

pub(crate) async fn save(
    db_context: &mut DbContext,
    media_folder: &MediaFolder,
    radarr_movie: RadarrMovie,
    inspected: Option<Inspected>,
) -> Result<(), Error> {
    let folder = match &radarr_movie.movie_file {
        Some(file) => media_folder.read_movie_folder.call(&file.path).await,
        None => MovieFolder::default(),
    };
    let visible = inspected.as_ref().is_some_and(Inspected::on_disk);
    let date_added = radarr_movie.movie_file.as_ref().map(|x| x.date_added);
    let artwork = [
        (ArtworkKind::Poster, folder.poster),
        (ArtworkKind::Fanart, folder.fanart),
        (ArtworkKind::Clearlogo, folder.clearlogo),
    ]
    .into_iter()
    .filter_map(|(kind, file)| {
        Some(FoundArtwork {
            season_id: None,
            episode_id: None,
            kind,
            file: file?,
        })
    })
    .collect::<Vec<_>>();
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let db = transaction.executor_mut();
    let (media_file_id, media_file_changed) = match (&radarr_movie.movie_file, inspected) {
        (Some(file), Some(inspected)) => {
            let (id, changed) = media_files::save(db, &file.path, inspected)
                .await
                .map_err(Error::caused_by)?;
            (Some(id), changed)
        }
        _ => (None, false),
    };
    let stored = Movie::filter_by_tmdb_id(radarr_movie.tmdb_id)
        .first()
        .exec(db)
        .await
        .map_err(Error::caused_by)?;
    let nfo = Nfo::read(
        folder.nfo,
        stored.as_ref().and_then(|x| x.nfo_modified_at),
        |path| media_folder.read_movie_nfo.call(path),
    )
    .await;
    let metadata = Metadata::of(&nfo, &radarr_movie, stored.as_ref());
    let had_nfo = stored.as_ref().is_some_and(|x| x.nfo_modified_at.is_some());
    let (movie_id, change) = match stored {
        Some(mut movie) => {
            let added_at = movie.added_at.or(date_added);
            let old_media_file_id = movie.media_file_id;
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
                media_files::delete(db, vec![old_media_file_id])
                    .await
                    .map_err(Error::caused_by)?;
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
    let owner = Owner::Movie(movie_id);
    if !matches!(nfo, Nfo::Unchanged) {
        let actors = match &nfo {
            Nfo::Read(nfo, _) => nfo.actors.as_slice(),
            _ => &[],
        };
        metadata::replace_actors(db, owner, had_nfo, actors)
            .await
            .map_err(Error::caused_by)?;
    }
    let artwork_changed = metadata::save_artwork(db, owner, artwork)
        .await
        .map_err(Error::caused_by)?;
    let unreadable_nfos = nfo
        .unreadable_path()
        .map(|path| (None, path.to_owned()))
        .into_iter()
        .collect::<HashSet<_>>();
    metadata::save_unreadable_nfos(db, owner, Vec::new(), unreadable_nfos)
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
        let db = transaction.executor_mut();
        let media_file_id = movie.media_file_id;
        metadata::delete(db, Owner::Movie(movie.id))
            .await
            .map_err(Error::caused_by)?;
        movie.delete().exec(db).await.map_err(Error::caused_by)?;
        if let Some(media_file_id) = media_file_id {
            media_files::delete(db, vec![media_file_id])
                .await
                .map_err(Error::caused_by)?;
        }
        transaction.commit().await.map_err(Error::caused_by)?;
        library_items::count(Source::Radarr, Change::Removed);
    }
    Ok(())
}
