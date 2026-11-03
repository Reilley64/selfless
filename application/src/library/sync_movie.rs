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
use crate::library::metadata::FoundArtwork;
use crate::library::metadata::Nfo;
use crate::library::metadata::Owner;
use crate::ports::FindMediaFile;
use crate::ports::ListSubtitleFiles;
use crate::ports::RadarrMovie;
use crate::ports::ReadMovieFolder;
use crate::ports::ReadMovieNfo;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use domain::MediaFile;
use domain::Movie;
use domain::MovieMetadata;
use domain::MovieNfo;
use domain::Source;
use jiff::Timestamp;
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
                let stored_file = match &radarr_movie.movie_file {
                    Some(file) => db_context
                        .media_files()
                        .filter(MediaFile::fields().path().eq(&file.path))
                        .first()
                        .exec(db_context.executor_mut())
                        .await
                        .map_err(Error::caused_by)?,
                    None => None,
                };
                let stored_nfo_modified_at = db_context
                    .movies()
                    .filter(Movie::fields().tmdb_id().eq(radarr_movie.tmdb_id))
                    .first()
                    .exec(db_context.executor_mut())
                    .await
                    .map_err(Error::caused_by)?
                    .and_then(|x| x.nfo_modified_at);
                let inspected = inspect(
                    &files,
                    stored_file.as_ref(),
                    stored_nfo_modified_at,
                    &radarr_movie,
                )
                .await?;
                save(&mut db_context, radarr_movie, inspected).await
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

pub(crate) struct InspectedMovie {
    media_file: Option<Inspected>,
    artwork: Vec<FoundArtwork>,
    nfo: Nfo<MovieNfo>,
}

pub(crate) async fn inspect(
    files: &MovieFiles,
    stored_file: Option<&MediaFile>,
    stored_nfo_modified_at: Option<Timestamp>,
    radarr_movie: &RadarrMovie,
) -> Result<InspectedMovie, Error> {
    let Some(file) = &radarr_movie.movie_file else {
        return Ok(InspectedMovie {
            media_file: None,
            artwork: Vec::new(),
            nfo: Nfo::Missing,
        });
    };
    let media_file = media_files::inspect(
        &files.find_media_file,
        &files.list_subtitle_files,
        &files.prober,
        stored_file,
        &file.path,
    )
    .await
    .map_err(Error::caused_by)?;
    let folder = files.read_movie_folder.call(&file.path).await;
    let nfo = Nfo::read(folder.nfo, stored_nfo_modified_at, |path| {
        files.read_movie_nfo.call(path)
    })
    .await;
    Ok(InspectedMovie {
        media_file: Some(media_file),
        artwork: metadata::item_artwork(folder.poster, folder.fanart, folder.clearlogo),
        nfo,
    })
}

pub(crate) async fn save(
    db_context: &mut DbContext,
    radarr_movie: RadarrMovie,
    inspected: InspectedMovie,
) -> Result<(), Error> {
    let InspectedMovie {
        media_file,
        artwork,
        nfo,
    } = inspected;
    let visible = Movie::in_library(
        radarr_movie.movie_file.is_some(),
        media_file.as_ref().is_some_and(Inspected::on_disk),
    );
    let date_added = radarr_movie.movie_file.as_ref().map(|x| x.date_added);
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let (media_file_id, media_file_changed) = match (&radarr_movie.movie_file, media_file) {
        (Some(file), Some(inspected)) => {
            let saved = media_files::save(&mut transaction, &file.path, inspected)
                .await
                .map_err(Error::caused_by)?;
            (Some(saved.id), saved.changed)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::source_health::ApiCallResult;
    use crate::ports::FoundFile;
    use crate::ports::RadarrMovieFile;
    use crate::test::MOVIE_FILE;
    use crate::test::MovieLibrary;
    use crate::test::arrival;
    use crate::test::at;
    use domain::Actor;
    use domain::Artwork;
    use domain::ArtworkKind;
    use domain::MovieNfo;
    use domain::NfoActor;
    use domain::SyncProblem;
    use domain::SyncProblemKind;
    use jiff::Timestamp;
    use jiff::civil::date;
    use uuid::Uuid;

    const UPGRADED_FILE: &str = "/media/Movies/Arrival (2016)/Arrival (2016) Remux-2160p.mkv";
    const FOLDER: &str = "/media/Movies/Arrival (2016)";
    const NFO: &str = "/media/Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.nfo";

    async fn movie(library: &MovieLibrary) -> Movie {
        let [movie] = movies(library).await.try_into().unwrap();
        movie
    }

    async fn movies(library: &MovieLibrary) -> Vec<Movie> {
        let mut db_context = library.db_context.clone();
        db_context
            .movies()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
    }

    async fn actors(library: &MovieLibrary) -> Vec<(String, Option<String>, Option<i64>)> {
        let mut db_context = library.db_context.clone();
        let mut actors = db_context
            .actors()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x: Actor| (x.name, x.role, x.order))
            .collect::<Vec<_>>();
        actors.sort_by_key(|x| x.2);
        actors
    }

    async fn artwork(library: &MovieLibrary) -> Vec<(ArtworkKind, String, Timestamp)> {
        let mut db_context = library.db_context.clone();
        let mut artwork = db_context
            .artwork()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x: Artwork| (x.kind, x.path, x.modified_at))
            .collect::<Vec<_>>();
        artwork.sort_by(|a, b| a.1.cmp(&b.1));
        artwork
    }

    async fn problems(library: &MovieLibrary) -> Vec<SyncProblem> {
        let mut db_context = library.db_context.clone();
        db_context
            .sync_problems()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
    }

    async fn media_files(library: &MovieLibrary) -> Vec<MediaFile> {
        let mut db_context = library.db_context.clone();
        db_context
            .media_files()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
    }

    fn found(path: &str, modified_at: &str) -> Option<FoundFile> {
        Some(FoundFile {
            path: path.to_owned(),
            modified_at: at(modified_at),
        })
    }

    fn nfo() -> MovieNfo {
        MovieNfo {
            title: Some("Arrival".to_owned()),
            sort_title: Some("arrival".to_owned()),
            original_title: Some("Story of Your Life".to_owned()),
            year: Some(2016),
            plot: Some("Linguistics professor Louise Banks interprets the visitors.".to_owned()),
            runtime_minutes: Some(116),
            certification: Some("PG-13".to_owned()),
            premiered: Some(date(2016, 11, 10)),
            studio: Some("FilmNation Entertainment".to_owned()),
            genres: vec!["Mystery".to_owned()],
            rating: Some(7.6),
            directors: vec!["Denis Villeneuve".to_owned()],
            actors: vec![
                nfo_actor("Amy Adams", "Louise Banks", 0),
                nfo_actor("Jeremy Renner", "Ian Donnelly", 1),
                nfo_actor("Forest Whitaker", "Colonel Weber", 2),
            ],
        }
    }

    fn nfo_actor(name: &str, role: &str, order: i64) -> NfoActor {
        NfoActor {
            name: name.to_owned(),
            role: Some(role.to_owned()),
            order: Some(order),
        }
    }

    fn with_nfo(library: &MovieLibrary, nfo: Option<MovieNfo>, modified_at: &str) {
        let mut sources = library.sources();
        sources.folder.nfo = found(NFO, modified_at);
        sources.nfo = nfo;
    }

    #[tokio::test]
    async fn a_radarr_movie_whose_file_is_on_disk_is_stored_visible_with_radarrs_fields() {
        let library = MovieLibrary::new().await;

        library.sync_movie().await.unwrap();

        let movie = movie(&library).await;
        assert!(movie.visible);
        assert_eq!(movie.radarr_id, 42);
        assert_eq!(movie.tmdb_id, 329_865);
        assert_eq!(movie.title, "Arrival");
        assert_eq!(movie.year, 2016);
        assert_eq!(
            movie.overview.as_deref(),
            Some("An expert linguist is recruited by the military.")
        );
        assert_eq!(movie.genres, ["Drama", "Science Fiction"]);
        assert_eq!(movie.added_at, Some(at("2026-10-29T10:00:00Z")));
    }

    #[tokio::test]
    async fn a_movie_without_a_file_is_hidden_and_a_file_again_shows_it_with_the_same_id() {
        let library = MovieLibrary::new().await;
        library.sync_movie().await.unwrap();
        let before = movie(&library).await;

        library.sources().radarr_movie = Some(arrival(None));
        library.sync_movie().await.unwrap();
        let hidden = movie(&library).await;
        library.sources().radarr_movie = Some(arrival(Some(MOVIE_FILE)));
        library.sync_movie().await.unwrap();

        assert!(!hidden.visible);
        let after = movie(&library).await;
        assert!(after.visible);
        assert_eq!(after.id, before.id);
    }

    #[tokio::test]
    async fn a_movie_whose_file_is_missing_on_disk_is_hidden() {
        let library = MovieLibrary::new().await;
        library.sources().files_on_disk.clear();

        library.sync_movie().await.unwrap();

        assert!(!movie(&library).await.visible);
    }

    #[tokio::test]
    async fn an_upgraded_file_keeps_the_movie_and_its_added_date() {
        let library = MovieLibrary::new().await;
        library.sync_movie().await.unwrap();
        let before = movie(&library).await;

        library.put_file(UPGRADED_FILE, 2_000, "2026-10-31T09:00:00Z");
        library.sources().radarr_movie = Some(RadarrMovie {
            movie_file: Some(RadarrMovieFile {
                path: UPGRADED_FILE.to_owned(),
                date_added: at("2026-10-31T10:00:00Z"),
            }),
            ..arrival(None)
        });
        library.sync_movie().await.unwrap();

        let after = movie(&library).await;
        assert_eq!(after.id, before.id);
        assert_eq!(after.added_at, Some(at("2026-10-29T10:00:00Z")));
        assert!(after.visible);
    }

    #[tokio::test]
    async fn a_movie_radarr_no_longer_has_is_removed_with_its_actors_artwork_problems_and_media_file()
     {
        let library = MovieLibrary::new().await;
        with_nfo(&library, Some(nfo()), "2026-10-29T11:00:00Z");
        library.sources().folder.poster =
            found(&format!("{FOLDER}/poster.jpg"), "2026-10-29T11:00:00Z");
        library.sources().unprobeable.insert(MOVIE_FILE.to_owned());
        library.sync_movie().await.unwrap();
        let before = movie(&library).await;
        let mut db_context = library.db_context.clone();
        db_context
            .sync_problems()
            .create()
            .movie_id(Some(before.id))
            .kind(SyncProblemKind::UnreadableNfo)
            .path(NFO)
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        library.sources().radarr_movie = None;
        library.sync_movie().await.unwrap();

        assert!(movies(&library).await.is_empty());
        assert!(actors(&library).await.is_empty());
        assert!(artwork(&library).await.is_empty());
        assert!(problems(&library).await.is_empty());
        assert!(media_files(&library).await.is_empty());
        library.sources().radarr_movie = Some(arrival(Some(MOVIE_FILE)));
        library.sync_movie().await.unwrap();
        assert_ne!(movie(&library).await.id, before.id);
    }

    #[tokio::test]
    async fn a_movie_stores_the_fields_and_actors_of_its_nfo() {
        let library = MovieLibrary::new().await;
        with_nfo(&library, Some(nfo()), "2026-10-29T11:00:00Z");

        library.sync_movie().await.unwrap();

        let movie = movie(&library).await;
        assert_eq!(movie.title, "Arrival");
        assert_eq!(movie.sort_title.as_deref(), Some("arrival"));
        assert_eq!(movie.original_title.as_deref(), Some("Story of Your Life"));
        assert_eq!(
            movie.overview.as_deref(),
            Some("Linguistics professor Louise Banks interprets the visitors.")
        );
        assert_eq!(movie.runtime_minutes, Some(116));
        assert_eq!(movie.certification.as_deref(), Some("PG-13"));
        assert_eq!(movie.premiered, Some(date(2016, 11, 10)));
        assert_eq!(movie.studio.as_deref(), Some("FilmNation Entertainment"));
        assert_eq!(movie.genres, ["Mystery"]);
        assert_eq!(movie.rating, Some(7.6));
        assert_eq!(movie.directors, ["Denis Villeneuve"]);
        assert_eq!(movie.nfo_modified_at, Some(at("2026-10-29T11:00:00Z")));
        assert_eq!(
            actors(&library).await,
            [
                (
                    "Amy Adams".to_owned(),
                    Some("Louise Banks".to_owned()),
                    Some(0)
                ),
                (
                    "Jeremy Renner".to_owned(),
                    Some("Ian Donnelly".to_owned()),
                    Some(1)
                ),
                (
                    "Forest Whitaker".to_owned(),
                    Some("Colonel Weber".to_owned()),
                    Some(2)
                ),
            ]
        );
    }

    #[tokio::test]
    async fn a_movie_without_an_nfo_records_no_sync_problem() {
        let library = MovieLibrary::new().await;

        library.sync_movie().await.unwrap();

        assert_eq!(movie(&library).await.sort_title, None);
        assert!(problems(&library).await.is_empty());
    }

    #[tokio::test]
    async fn an_unreadable_nfo_stores_radarrs_fields_and_an_unreadable_nfo_problem_owned_by_the_movie()
     {
        let library = MovieLibrary::new().await;
        with_nfo(&library, None, "2026-10-29T11:00:00Z");

        library.sync_movie().await.unwrap();

        let movie = movie(&library).await;
        assert_eq!(
            movie.overview.as_deref(),
            Some("An expert linguist is recruited by the military.")
        );
        assert_eq!(movie.sort_title, None);
        assert!(actors(&library).await.is_empty());
        let problems = problems(&library)
            .await
            .into_iter()
            .map(|x| (x.kind, x.movie_id, x.media_file_id, x.path))
            .collect::<Vec<_>>();
        assert_eq!(
            problems,
            [(
                SyncProblemKind::UnreadableNfo,
                Some(movie.id),
                None::<Uuid>,
                NFO.to_owned()
            )]
        );
    }

    #[tokio::test]
    async fn the_artwork_in_the_movie_folder_is_stored_with_its_kind_and_modification_time() {
        let library = MovieLibrary::new().await;
        {
            let mut sources = library.sources();
            sources.folder.poster = found(&format!("{FOLDER}/poster.jpg"), "2026-10-28T12:30:00Z");
            sources.folder.fanart = found(&format!("{FOLDER}/fanart.jpg"), "2026-10-28T12:31:00Z");
            sources.folder.clearlogo =
                found(&format!("{FOLDER}/clearlogo.png"), "2026-10-28T12:32:00Z");
        }

        library.sync_movie().await.unwrap();

        assert_eq!(
            artwork(&library).await,
            [
                (
                    ArtworkKind::Clearlogo,
                    format!("{FOLDER}/clearlogo.png"),
                    at("2026-10-28T12:32:00Z")
                ),
                (
                    ArtworkKind::Fanart,
                    format!("{FOLDER}/fanart.jpg"),
                    at("2026-10-28T12:31:00Z")
                ),
                (
                    ArtworkKind::Poster,
                    format!("{FOLDER}/poster.jpg"),
                    at("2026-10-28T12:30:00Z")
                ),
            ]
        );
    }

    #[tokio::test]
    async fn a_newer_nfo_is_read_again_and_replaces_the_actors() {
        let library = MovieLibrary::new().await;
        with_nfo(&library, Some(nfo()), "2026-10-29T11:00:00Z");
        library.sync_movie().await.unwrap();

        let directors_cut = MovieNfo {
            title: Some("Arrival (Director's Cut)".to_owned()),
            ..nfo()
        };
        with_nfo(&library, Some(directors_cut), "2026-10-30T08:00:00Z");
        library.sync_movie().await.unwrap();

        assert_eq!(movie(&library).await.title, "Arrival (Director's Cut)");
        assert_eq!(actors(&library).await.len(), 3);
        assert_eq!(library.sources().nfo_reads, 2);
    }

    #[tokio::test]
    async fn an_nfo_with_the_stored_modification_time_is_not_read_again() {
        let library = MovieLibrary::new().await;
        with_nfo(&library, Some(nfo()), "2026-10-29T11:00:00Z");
        library.sync_movie().await.unwrap();

        let directors_cut = MovieNfo {
            title: Some("Arrival (Director's Cut)".to_owned()),
            ..nfo()
        };
        with_nfo(&library, Some(directors_cut), "2026-10-29T11:00:00Z");
        library.sync_movie().await.unwrap();

        assert_eq!(movie(&library).await.title, "Arrival");
        assert_eq!(actors(&library).await.len(), 3);
        assert_eq!(library.sources().nfo_reads, 1);
    }

    #[tokio::test]
    async fn the_artwork_is_replaced_when_the_folder_changes() {
        let library = MovieLibrary::new().await;
        {
            let mut sources = library.sources();
            sources.folder.poster = found(&format!("{FOLDER}/poster.jpg"), "2026-10-28T12:30:00Z");
            sources.folder.fanart = found(&format!("{FOLDER}/fanart.jpg"), "2026-10-28T12:31:00Z");
        }
        library.sync_movie().await.unwrap();

        {
            let mut sources = library.sources();
            sources.folder.poster = found(&format!("{FOLDER}/poster.jpg"), "2026-10-30T08:00:00Z");
            sources.folder.fanart = None;
            sources.folder.clearlogo =
                found(&format!("{FOLDER}/clearlogo.png"), "2026-10-30T08:01:00Z");
        }
        library.sync_movie().await.unwrap();

        assert_eq!(
            artwork(&library).await,
            [
                (
                    ArtworkKind::Clearlogo,
                    format!("{FOLDER}/clearlogo.png"),
                    at("2026-10-30T08:01:00Z")
                ),
                (
                    ArtworkKind::Poster,
                    format!("{FOLDER}/poster.jpg"),
                    at("2026-10-30T08:00:00Z")
                ),
            ]
        );
    }

    #[tokio::test]
    async fn a_failed_radarr_call_is_recorded_in_the_source_health() {
        let library = MovieLibrary::new().await;
        library.sources().radarr_is_down = true;

        let synced = library.sync_movie().await;

        assert!(synced.is_err());
        assert!(matches!(
            library.source_health.health(Source::Radarr).last_api_call,
            Some(ApiCallResult::Failed(_))
        ));
    }
}
