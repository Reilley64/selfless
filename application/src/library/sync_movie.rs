mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::library::sync::movies;
use crate::library::sync::movies::MovieFiles;
use crate::ports::persistence::DbScope;
use domain::MediaFile;
use domain::Movie;
use domain::RadarrId;
use domain::Source;
use tracing::Instrument;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    radarr_id: RadarrId,
) -> Result<Output, Error> {
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
        let source_movie = get_radarr_movie.call(radarr_id).await;
        match source_health
            .api_call(Source::Radarr, source_movie)
            .map_err(Error::caused_by)?
        {
            Some(source_movie) => {
                let stored_file = match &source_movie.media_file {
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
                    .filter(Movie::fields().tmdb_id().eq(source_movie.tmdb_id))
                    .first()
                    .exec(db_context.executor_mut())
                    .await
                    .map_err(Error::caused_by)?
                    .and_then(|x| x.nfo_modified_at);
                let inspected = movies::inspect(
                    &files,
                    stored_file.as_ref(),
                    stored_nfo_modified_at,
                    &source_movie,
                )
                .await
                .map_err(Error::caused_by)?;
                movies::save(&mut db_context, source_movie, inspected)
                    .await
                    .map_err(Error::caused_by)
            }
            None => {
                let movies = db_context
                    .movies()
                    .filter(Movie::fields().radarr_id().eq(radarr_id))
                    .exec(db_context.executor_mut())
                    .await
                    .map_err(Error::caused_by)?;
                movies::remove(&mut db_context, movies)
                    .await
                    .map_err(Error::caused_by)
            }
        }
    }
    .instrument(movies::span())
    .await?;
    Ok(Output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::source_health::ApiCallResult;
    use crate::ports::FoundFile;
    use crate::ports::SourceMediaFile;
    use crate::ports::SourceMovie;
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
    use domain::TmdbId;
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
            overview: Some(
                "Linguistics professor Louise Banks interprets the visitors.".to_owned(),
            ),
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
    async fn a_movie_whose_media_file_is_on_disk_is_stored_in_the_library_with_radarrs_fields() {
        let library = MovieLibrary::new().await;

        library.sync_movie().await.unwrap();

        let movie = movie(&library).await;
        assert!(movie.in_library);
        assert_eq!(movie.radarr_id, RadarrId(42));
        assert_eq!(movie.tmdb_id, TmdbId(329_865));
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

        library.sources().movie = Some(arrival(None));
        library.sync_movie().await.unwrap();
        let hidden = movie(&library).await;
        library.sources().movie = Some(arrival(Some(MOVIE_FILE)));
        library.sync_movie().await.unwrap();

        assert!(!hidden.in_library);
        let after = movie(&library).await;
        assert!(after.in_library);
        assert_eq!(after.id, before.id);
    }

    #[tokio::test]
    async fn a_movie_whose_file_is_missing_on_disk_is_hidden() {
        let library = MovieLibrary::new().await;
        library.sources().files_on_disk.clear();

        library.sync_movie().await.unwrap();

        assert!(!movie(&library).await.in_library);
    }

    #[tokio::test]
    async fn an_upgraded_file_keeps_the_movie_and_its_added_date() {
        let library = MovieLibrary::new().await;
        library.sync_movie().await.unwrap();
        let before = movie(&library).await;

        library.put_file(UPGRADED_FILE, 2_000, "2026-10-31T09:00:00Z");
        library.sources().movie = Some(SourceMovie {
            media_file: Some(SourceMediaFile {
                path: UPGRADED_FILE.to_owned(),
                added_at: at("2026-10-31T10:00:00Z"),
            }),
            ..arrival(None)
        });
        library.sync_movie().await.unwrap();

        let after = movie(&library).await;
        assert_eq!(after.id, before.id);
        assert_eq!(after.added_at, Some(at("2026-10-29T10:00:00Z")));
        assert!(after.in_library);
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

        library.sources().movie = None;
        library.sync_movie().await.unwrap();

        assert!(movies(&library).await.is_empty());
        assert!(actors(&library).await.is_empty());
        assert!(artwork(&library).await.is_empty());
        assert!(problems(&library).await.is_empty());
        assert!(media_files(&library).await.is_empty());
        library.sources().movie = Some(arrival(Some(MOVIE_FILE)));
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

    async fn unreadable_file_problems(library: &MovieLibrary) -> Vec<(Option<Uuid>, String)> {
        problems(library)
            .await
            .into_iter()
            .filter(|x| x.kind == SyncProblemKind::UnreadableFile)
            .map(|x| (x.movie_id, x.path))
            .collect()
    }

    fn arrival_extended() -> SourceMovie {
        SourceMovie {
            title: "Arrival (Extended)".to_owned(),
            ..arrival(Some(MOVIE_FILE))
        }
    }

    #[tokio::test]
    async fn an_unreadable_media_file_keeps_the_stored_movie_and_records_an_unreadable_file_until_it_is_read()
     {
        let library = MovieLibrary::new().await;
        library.sources().folder.poster =
            found(&format!("{FOLDER}/poster.jpg"), "2026-10-29T11:00:00Z");
        library.sync_movie().await.unwrap();
        let before = movie(&library).await;

        {
            let mut sources = library.sources();
            sources.unreadable_files.insert(MOVIE_FILE.to_owned());
            sources.folder.poster = None;
            sources.movie = Some(arrival_extended());
        }
        library.sync_movie().await.unwrap();
        let kept = movie(&library).await;
        let kept_artwork = artwork(&library).await.len();
        let recorded = unreadable_file_problems(&library).await;
        library.sources().unreadable_files.clear();
        library.sync_movie().await.unwrap();

        assert_eq!(kept.id, before.id);
        assert!(kept.in_library);
        assert_eq!(kept.title, "Arrival");
        assert_eq!(kept.media_file_id, before.media_file_id);
        assert_eq!(kept_artwork, 1);
        assert_eq!(recorded, [(Some(before.id), MOVIE_FILE.to_owned())]);
        assert_eq!(movie(&library).await.title, "Arrival (Extended)");
        assert!(unreadable_file_problems(&library).await.is_empty());
    }

    #[tokio::test]
    async fn an_unreadable_file_in_the_movie_folder_keeps_the_stored_artwork_and_metadata() {
        let library = MovieLibrary::new().await;
        let poster = format!("{FOLDER}/poster.jpg");
        library.sources().folder.poster = found(&poster, "2026-10-29T11:00:00Z");
        library.sync_movie().await.unwrap();

        {
            let mut sources = library.sources();
            sources.unreadable_files.insert(poster.clone());
            sources.folder.poster = found(&poster, "2026-10-30T08:00:00Z");
            sources.movie = Some(arrival_extended());
        }
        library.sync_movie().await.unwrap();

        let movie = movie(&library).await;
        assert!(movie.in_library);
        assert_eq!(movie.title, "Arrival");
        assert_eq!(
            artwork(&library).await,
            [(
                ArtworkKind::Poster,
                poster.clone(),
                at("2026-10-29T11:00:00Z")
            )]
        );
        assert_eq!(
            unreadable_file_problems(&library).await,
            [(Some(movie.id), poster)]
        );
    }

    #[tokio::test]
    async fn a_new_movie_whose_media_file_is_unreadable_is_stored_hidden_with_an_unreadable_file() {
        let library = MovieLibrary::new().await;
        library
            .sources()
            .unreadable_files
            .insert(MOVIE_FILE.to_owned());

        library.sync_movie().await.unwrap();

        let movie = movie(&library).await;
        assert!(!movie.in_library);
        assert_eq!(movie.media_file_id, None);
        assert_eq!(
            unreadable_file_problems(&library).await,
            [(Some(movie.id), MOVIE_FILE.to_owned())]
        );
    }

    #[tokio::test]
    async fn an_nfo_that_cannot_be_read_keeps_the_stored_nfo_metadata_and_records_an_unreadable_file()
     {
        let library = MovieLibrary::new().await;
        with_nfo(&library, Some(nfo()), "2026-10-29T11:00:00Z");
        library.sync_movie().await.unwrap();

        let directors_cut = MovieNfo {
            title: Some("Arrival (Director's Cut)".to_owned()),
            ..nfo()
        };
        with_nfo(&library, Some(directors_cut), "2026-10-30T08:00:00Z");
        library.sources().unreadable_files.insert(NFO.to_owned());
        library.sync_movie().await.unwrap();

        let movie = movie(&library).await;
        assert_eq!(movie.title, "Arrival");
        assert_eq!(movie.sort_title.as_deref(), Some("arrival"));
        assert_eq!(movie.nfo_modified_at, Some(at("2026-10-29T11:00:00Z")));
        assert_eq!(actors(&library).await.len(), 3);
        assert_eq!(
            problems(&library)
                .await
                .into_iter()
                .map(|x| (x.kind, x.movie_id, x.path))
                .collect::<Vec<_>>(),
            [(
                SyncProblemKind::UnreadableFile,
                Some(movie.id),
                NFO.to_owned()
            )]
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
