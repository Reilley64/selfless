mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::library::sync::shows;
use crate::library::sync::shows::ShowFiles;
use crate::ports::persistence::DbScope;
use domain::Show;
use domain::SonarrId;
use domain::Source;
use tracing::Instrument;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    sonarr_id: SonarrId,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        find_media_file,
        get_sonarr_show,
        list_subtitle_files,
        prober,
        read_episode_folder,
        read_episode_nfo,
        read_show_folder,
        read_show_nfo,
        source_health,
    } = dependencies.into();
    let files = ShowFiles {
        find_media_file,
        list_subtitle_files,
        prober,
        read_episode_folder,
        read_episode_nfo,
        read_show_folder,
        read_show_nfo,
    };
    async {
        let source_show = get_sonarr_show.call(sonarr_id).await;
        match source_health
            .api_call(Source::Sonarr, source_show)
            .map_err(Error::caused_by)?
        {
            Some(source_show) => {
                let inspected = shows::inspect(&mut db_context, &files, &source_show)
                    .await
                    .map_err(Error::caused_by)?;
                shows::save(&mut db_context, source_show, inspected)
                    .await
                    .map_err(Error::caused_by)
            }
            None => match db_context
                .shows()
                .filter(Show::fields().sonarr_id().eq(sonarr_id))
                .first()
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?
            {
                Some(show) => shows::remove(&mut db_context, show)
                    .await
                    .map_err(Error::caused_by),
                None => Ok(()),
            },
        }
    }
    .instrument(shows::span())
    .await?;
    Ok(Output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::FoundFile;
    use crate::ports::SourceEpisode;
    use crate::test::S01E01_FILE;
    use crate::test::S01E02_E03_FILE;
    use crate::test::S02E01_FILE;
    use crate::test::SHOW_FOLDER;
    use crate::test::SPECIAL_FILE;
    use crate::test::ShowLibrary;
    use crate::test::at;
    use crate::test::severance;
    use crate::test::source_media_file;
    use domain::Actor;
    use domain::Artwork;
    use domain::ArtworkKind;
    use domain::Episode;
    use domain::EpisodeNfo;
    use domain::MediaFile;
    use domain::NfoActor;
    use domain::ShowNfo;
    use domain::SyncProblem;
    use domain::SyncProblemKind;
    use domain::TvdbId;
    use jiff::civil::date;
    use std::collections::HashMap;
    use uuid::Uuid;

    const UPGRADED_S02E01_FILE: &str =
        "/media/TV/Severance/Season 02/Severance - S02E01 - Hello, Ms. Cobel Bluray-2160p.mkv";
    const SHOW_NFO: &str = "/media/TV/Severance/tvshow.nfo";

    async fn show(library: &ShowLibrary) -> Show {
        let [show] = shows(library).await.try_into().unwrap();
        show
    }

    async fn shows(library: &ShowLibrary) -> Vec<Show> {
        let mut db_context = library.db_context.clone();
        db_context
            .shows()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
    }

    async fn season_numbers(library: &ShowLibrary) -> HashMap<Uuid, i64> {
        let mut db_context = library.db_context.clone();
        db_context
            .seasons()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| (x.id, x.number))
            .collect()
    }

    async fn episodes(library: &ShowLibrary) -> Vec<(i64, Episode)> {
        let mut db_context = library.db_context.clone();
        let season_numbers = season_numbers(library).await;
        let mut episodes = db_context
            .episodes()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| (season_numbers[&x.season_id], x))
            .collect::<Vec<_>>();
        episodes.sort_by_key(|(season, x)| (*season, x.number));
        episodes
    }

    async fn episode(library: &ShowLibrary, season: i64, number: i64) -> Episode {
        episodes(library)
            .await
            .into_iter()
            .find(|(s, x)| *s == season && x.number == number)
            .unwrap()
            .1
    }

    async fn titles_and_ratings(
        library: &ShowLibrary,
        season: i64,
    ) -> Vec<(i64, String, Option<f64>)> {
        episodes(library)
            .await
            .into_iter()
            .filter(|(s, _)| *s == season)
            .map(|(_, x)| (x.number, x.title, x.rating))
            .collect()
    }

    async fn actors(library: &ShowLibrary) -> Vec<(String, Option<String>, Option<i64>)> {
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

    async fn artwork(library: &ShowLibrary) -> Vec<Artwork> {
        let mut db_context = library.db_context.clone();
        let mut artwork = db_context
            .artwork()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        artwork.sort_by(|a, b| (&a.path, a.episode_id).cmp(&(&b.path, b.episode_id)));
        artwork
    }

    async fn problems(library: &ShowLibrary) -> Vec<SyncProblem> {
        let mut db_context = library.db_context.clone();
        db_context
            .sync_problems()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
    }

    async fn media_files(library: &ShowLibrary) -> Vec<MediaFile> {
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

    fn nfo_path(media_file: &str) -> String {
        media_file.replace(".mkv", ".nfo")
    }

    fn show_nfo() -> ShowNfo {
        ShowNfo {
            title: Some("Severance".to_owned()),
            sort_title: Some("severance".to_owned()),
            overview: Some("Mark leads a team whose memories are surgically divided.".to_owned()),
            certification: Some("TV-MA".to_owned()),
            premiered: Some(date(2022, 2, 18)),
            end_date: None,
            status: Some("Continuing".to_owned()),
            network: Some("Apple TV+".to_owned()),
            genres: vec!["Drama".to_owned(), "Thriller".to_owned()],
            rating: Some(8.7),
            actors: vec![
                nfo_actor("Adam Scott", "Mark Scout", 0),
                nfo_actor("Britt Lower", "Helly R.", 1),
                nfo_actor("Zach Cherry", "Dylan George", 2),
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

    fn episode_nfo(
        sonarr_id: Option<i64>,
        season_number: i64,
        episode_number: i64,
        title: &str,
        rating: f64,
    ) -> EpisodeNfo {
        EpisodeNfo {
            sonarr_id: sonarr_id.map(SonarrId),
            title: Some(title.to_owned()),
            season_number: Some(season_number),
            episode_number: Some(episode_number),
            air_date: None,
            overview: Some(format!("The NFO's overview of {title}")),
            rating: Some(rating),
        }
    }

    fn with_show_nfo(library: &ShowLibrary, nfo: Option<ShowNfo>, modified_at: &str) {
        let mut sources = library.sources();
        sources.folder.nfo = found(SHOW_NFO, modified_at);
        sources.nfo = nfo;
    }

    fn with_episode_nfo(
        library: &ShowLibrary,
        media_file: &str,
        roots: Option<Vec<EpisodeNfo>>,
        modified_at: &str,
    ) {
        let mut sources = library.sources();
        let path = nfo_path(media_file);
        sources
            .episode_folders
            .entry(media_file.to_owned())
            .or_default()
            .nfo = found(&path, modified_at);
        match roots {
            Some(roots) => sources.episode_nfos.insert(path, roots),
            None => sources.episode_nfos.remove(&path),
        };
    }

    fn with_source_episodes(library: &ShowLibrary, change: impl FnOnce(&mut Vec<SourceEpisode>)) {
        let mut sources = library.sources();
        change(&mut sources.show.as_mut().unwrap().episodes);
    }

    fn severance_episodes(library: &ShowLibrary) -> Vec<SourceEpisode> {
        library.sources().show.as_ref().unwrap().episodes.clone()
    }

    fn source_episode(
        episodes: &mut [SourceEpisode],
        season: i64,
        number: i64,
    ) -> &mut SourceEpisode {
        episodes
            .iter_mut()
            .find(|x| x.season_number == season && x.episode_number == number)
            .unwrap()
    }

    async fn unreadable_file_problems(library: &ShowLibrary) -> Vec<(Option<Uuid>, String)> {
        problems(library)
            .await
            .into_iter()
            .filter(|x| x.kind == SyncProblemKind::UnreadableFile)
            .map(|x| (x.show_id, x.path))
            .collect()
    }

    fn retitle_show(library: &ShowLibrary) {
        library.sources().show.as_mut().unwrap().listing.title = "Severance (2022)".to_owned();
    }

    #[tokio::test]
    async fn an_unreadable_episode_file_keeps_the_stored_show_and_records_an_unreadable_file_until_it_is_read()
     {
        let library = ShowLibrary::new().await;
        library.sync_show().await.unwrap();
        let before = show(&library).await;

        {
            let mut sources = library.sources();
            sources.unreadable_files.insert(S01E01_FILE.to_owned());
            sources.files_on_disk.remove(S02E01_FILE);
        }
        retitle_show(&library);
        library.sync_show().await.unwrap();
        let kept = show(&library).await;
        let kept_in_library = episode(&library, 2, 1).await.in_library;
        let recorded = unreadable_file_problems(&library).await;
        library.sources().unreadable_files.clear();
        library.sync_show().await.unwrap();

        assert_eq!(kept.id, before.id);
        assert_eq!(kept.title, "Severance");
        assert!(kept_in_library);
        assert_eq!(recorded, [(Some(before.id), S01E01_FILE.to_owned())]);
        assert_eq!(show(&library).await.title, "Severance (2022)");
        assert!(!episode(&library, 2, 1).await.in_library);
        assert!(unreadable_file_problems(&library).await.is_empty());
    }

    #[tokio::test]
    async fn an_unreadable_file_beside_an_episode_keeps_the_stored_artwork() {
        let library = ShowLibrary::new().await;
        let thumb = S01E01_FILE.replace(".mkv", "-thumb.jpg");
        library
            .sources()
            .episode_folders
            .entry(S01E01_FILE.to_owned())
            .or_default()
            .thumb = found(&thumb, "2026-10-29T11:00:00Z");
        library.sync_show().await.unwrap();

        {
            let mut sources = library.sources();
            sources.unreadable_files.insert(thumb.clone());
            sources.episode_folders.get_mut(S01E01_FILE).unwrap().thumb =
                found(&thumb, "2026-10-30T08:00:00Z");
        }
        library.sync_show().await.unwrap();

        let show = show(&library).await;
        assert_eq!(
            artwork(&library)
                .await
                .into_iter()
                .map(|x| (x.path, x.modified_at))
                .collect::<Vec<_>>(),
            [(thumb.clone(), at("2026-10-29T11:00:00Z"))]
        );
        assert_eq!(
            unreadable_file_problems(&library).await,
            [(Some(show.id), thumb)]
        );
    }

    #[tokio::test]
    async fn an_unreadable_file_in_the_show_folder_keeps_the_stored_artwork_and_metadata() {
        let library = ShowLibrary::new().await;
        let poster = format!("{SHOW_FOLDER}/poster.jpg");
        library.sources().folder.poster = found(&poster, "2026-10-29T11:00:00Z");
        library.sync_show().await.unwrap();

        {
            let mut sources = library.sources();
            sources.unreadable_files.insert(poster.clone());
            sources.folder.poster = found(&poster, "2026-10-30T08:00:00Z");
        }
        retitle_show(&library);
        library.sync_show().await.unwrap();

        let show = show(&library).await;
        assert_eq!(show.title, "Severance");
        assert_eq!(
            artwork(&library)
                .await
                .into_iter()
                .map(|x| (x.path, x.modified_at))
                .collect::<Vec<_>>(),
            [(poster.clone(), at("2026-10-29T11:00:00Z"))]
        );
        assert_eq!(
            unreadable_file_problems(&library).await,
            [(Some(show.id), poster)]
        );
    }

    #[tokio::test]
    async fn a_show_nfo_that_cannot_be_read_keeps_the_stored_nfo_metadata_and_records_an_unreadable_file()
     {
        let library = ShowLibrary::new().await;
        with_show_nfo(&library, Some(show_nfo()), "2026-10-29T11:00:00Z");
        library.sync_show().await.unwrap();

        let retitled = ShowNfo {
            title: Some("Severance (NFO)".to_owned()),
            ..show_nfo()
        };
        with_show_nfo(&library, Some(retitled), "2026-10-30T08:00:00Z");
        library
            .sources()
            .unreadable_files
            .insert(SHOW_NFO.to_owned());
        library.sync_show().await.unwrap();

        let show = show(&library).await;
        assert_eq!(show.title, "Severance");
        assert_eq!(show.sort_title.as_deref(), Some("severance"));
        assert_eq!(show.nfo_modified_at, Some(at("2026-10-29T11:00:00Z")));
        assert_eq!(actors(&library).await.len(), 3);
        assert_eq!(
            problems(&library)
                .await
                .into_iter()
                .map(|x| (x.kind, x.show_id, x.path))
                .collect::<Vec<_>>(),
            [(
                SyncProblemKind::UnreadableFile,
                Some(show.id),
                SHOW_NFO.to_owned()
            )]
        );
    }

    #[tokio::test]
    async fn an_episode_nfo_that_cannot_be_read_keeps_the_stored_nfo_metadata_and_records_an_unreadable_file()
     {
        let library = ShowLibrary::new().await;
        with_episode_nfo(
            &library,
            S01E01_FILE,
            Some(vec![episode_nfo(
                Some(1011),
                1,
                1,
                "Good News About Hell",
                8.1,
            )]),
            "2026-10-29T11:00:00Z",
        );
        library.sync_show().await.unwrap();

        with_episode_nfo(
            &library,
            S01E01_FILE,
            Some(vec![episode_nfo(
                Some(1011),
                1,
                1,
                "Good News About Hell",
                9.0,
            )]),
            "2026-10-30T08:00:00Z",
        );
        library
            .sources()
            .unreadable_files
            .insert(nfo_path(S01E01_FILE));
        library.sync_show().await.unwrap();

        let episode = episode(&library, 1, 1).await;
        assert_eq!(episode.rating, Some(8.1));
        assert_eq!(episode.nfo_modified_at, Some(at("2026-10-29T11:00:00Z")));
        assert_eq!(
            problems(&library)
                .await
                .into_iter()
                .map(|x| (x.kind, x.show_id, x.path))
                .collect::<Vec<_>>(),
            [(
                SyncProblemKind::UnreadableFile,
                Some(show(&library).await.id),
                nfo_path(S01E01_FILE)
            )]
        );
    }

    #[tokio::test]
    async fn a_new_show_with_an_unreadable_file_is_stored_without_episodes_in_the_library_and_with_an_unreadable_file()
     {
        let library = ShowLibrary::new().await;
        library
            .sources()
            .unreadable_files
            .insert(S01E01_FILE.to_owned());

        library.sync_show().await.unwrap();

        let show = show(&library).await;
        assert!(episodes(&library).await.iter().all(|(_, x)| !x.in_library));
        assert!(media_files(&library).await.is_empty());
        assert_eq!(
            unreadable_file_problems(&library).await,
            [(Some(show.id), S01E01_FILE.to_owned())]
        );
    }

    #[tokio::test]
    async fn a_show_is_stored_with_sonarrs_fields_and_only_episodes_with_a_media_file_are_in_the_library()
     {
        let library = ShowLibrary::new().await;

        library.sync_show().await.unwrap();

        let show = show(&library).await;
        assert_eq!(show.sonarr_id, SonarrId(7));
        assert_eq!(show.tvdb_id, TvdbId(371_980));
        assert_eq!(show.title, "Severance");
        assert_eq!(show.year, Some(2022));
        assert_eq!(show.genres, ["Drama", "Mystery", "Science Fiction"]);
        assert_eq!(
            show.overview.as_deref(),
            Some("Mark leads a team of office workers.")
        );
        let mut seasons = season_numbers(&library)
            .await
            .into_values()
            .collect::<Vec<_>>();
        seasons.sort_unstable();
        assert_eq!(seasons, [0, 1, 2]);
        let episode = episode(&library, 2, 1).await;
        assert_eq!(episode.tvdb_id, TvdbId(10_535_541));
        assert_eq!(episode.title, "Hello, Ms. Cobel");
        assert_eq!(episode.air_date, Some(date(2025, 1, 17)));
        assert_eq!(
            episode.overview.as_deref(),
            Some("Mark leads his team on a search for answers.")
        );
        assert_eq!(episode.added_at, Some(at("2025-01-17T03:12:44Z")));
        assert_eq!(
            episodes(&library)
                .await
                .into_iter()
                .map(|(season, x)| (season, x.number, x.in_library))
                .collect::<Vec<_>>(),
            [
                (0, 1, true),
                (1, 1, true),
                (1, 2, true),
                (1, 3, true),
                (2, 1, true),
                (2, 2, false),
            ]
        );
    }

    #[tokio::test]
    async fn a_file_holding_two_episodes_gives_each_episode_its_own_row_pointing_at_the_one_file() {
        let library = ShowLibrary::new().await;

        library.sync_show().await.unwrap();

        let first = episode(&library, 1, 1).await;
        let second = episode(&library, 1, 2).await;
        let third = episode(&library, 1, 3).await;
        assert_ne!(second.id, third.id);
        assert!(second.media_file_id.is_some());
        assert_eq!(second.media_file_id, third.media_file_id);
        assert_ne!(first.media_file_id, second.media_file_id);
        assert_eq!(
            media_files(&library)
                .await
                .into_iter()
                .filter(|x| x.path == S01E02_E03_FILE)
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn an_episode_whose_file_is_missing_on_disk_is_not_in_the_library() {
        let library = ShowLibrary::new().await;
        library.sources().files_on_disk.remove(S01E01_FILE);

        library.sync_show().await.unwrap();

        assert!(!episode(&library, 1, 1).await.in_library);
        assert!(episode(&library, 1, 2).await.in_library);
    }

    #[tokio::test]
    async fn a_shared_file_missing_on_disk_records_one_missing_media_file_problem() {
        let library = ShowLibrary::new().await;
        library.sources().files_on_disk.remove(S01E02_E03_FILE);

        library.sync_show().await.unwrap();

        let media_file_id = episode(&library, 1, 2).await.media_file_id;
        assert_eq!(
            problems(&library)
                .await
                .into_iter()
                .map(|x| (x.kind, x.media_file_id, x.path))
                .collect::<Vec<_>>(),
            [(
                SyncProblemKind::MissingMediaFile,
                media_file_id,
                S01E02_E03_FILE.to_owned()
            )]
        );
    }

    #[tokio::test]
    async fn a_show_without_files_is_not_in_the_library_and_files_again_put_it_back_with_the_same_rows()
     {
        let library = ShowLibrary::new().await;
        library.sync_show().await.unwrap();
        let show_before = show(&library).await;
        let episodes_before = episodes(&library)
            .await
            .into_iter()
            .map(|(_, x)| x.id)
            .collect::<Vec<_>>();

        let with_files = severance_episodes(&library);
        with_source_episodes(&library, |episodes| {
            for episode in episodes {
                episode.media_file = None;
            }
        });
        library.sync_show().await.unwrap();
        let not_in_library = episodes(&library).await;
        with_source_episodes(&library, |episodes| *episodes = with_files);
        library.sync_show().await.unwrap();

        assert!(not_in_library.iter().all(|(_, x)| !x.in_library));
        assert_eq!(show(&library).await.id, show_before.id);
        let episodes_after = episodes(&library).await;
        assert_eq!(
            episodes_after.iter().map(|(_, x)| x.id).collect::<Vec<_>>(),
            episodes_before
        );
        assert!(episode(&library, 1, 1).await.in_library);
    }

    #[tokio::test]
    async fn a_show_whose_tvdb_id_changed_in_sonarr_keeps_its_rows_and_takes_the_new_tvdb_id() {
        let library = ShowLibrary::new().await;
        library.sync_show().await.unwrap();
        let show_before = show(&library).await;
        let episodes_before = episodes(&library)
            .await
            .into_iter()
            .map(|(_, x)| x.id)
            .collect::<Vec<_>>();

        library.sources().show.as_mut().unwrap().listing.tvdb_id = TvdbId(1_000_001);
        library.sync_show().await.unwrap();

        let show_after = show(&library).await;
        assert_eq!(show_after.id, show_before.id);
        assert_eq!(show_after.tvdb_id, TvdbId(1_000_001));
        assert_eq!(
            episodes(&library)
                .await
                .into_iter()
                .map(|(_, x)| x.id)
                .collect::<Vec<_>>(),
            episodes_before
        );
    }

    #[tokio::test]
    async fn an_upgraded_file_keeps_the_episode_and_its_added_date() {
        let library = ShowLibrary::new().await;
        library.sync_show().await.unwrap();
        let before = episode(&library, 2, 1).await;

        library
            .sources()
            .files_on_disk
            .insert(UPGRADED_S02E01_FILE.to_owned());
        with_source_episodes(&library, |episodes| {
            source_episode(episodes, 2, 1).media_file =
                source_media_file(UPGRADED_S02E01_FILE, "2026-10-31T10:00:00Z");
        });
        library.sync_show().await.unwrap();

        let after = episode(&library, 2, 1).await;
        assert_eq!(after.id, before.id);
        assert_eq!(after.added_at, Some(at("2025-01-17T03:12:44Z")));
        assert_ne!(after.media_file_id, before.media_file_id);
        assert!(after.in_library);
    }

    #[tokio::test]
    async fn a_show_sonarr_no_longer_has_is_removed_with_its_seasons_episodes_actors_artwork_problems_and_media_files()
     {
        let library = ShowLibrary::new().await;
        with_show_nfo(&library, Some(show_nfo()), "2026-10-29T11:00:00Z");
        with_episode_nfo(&library, S01E01_FILE, None, "2026-10-29T11:00:00Z");
        library.sources().folder.poster =
            found(&format!("{SHOW_FOLDER}/poster.jpg"), "2026-10-29T11:00:00Z");
        library.sources().files_on_disk.remove(S02E01_FILE);
        library.sync_show().await.unwrap();
        let before = show(&library).await;

        library.sources().show = None;
        library.sync_show().await.unwrap();

        assert!(shows(&library).await.is_empty());
        assert!(season_numbers(&library).await.is_empty());
        assert!(episodes(&library).await.is_empty());
        assert!(actors(&library).await.is_empty());
        assert!(artwork(&library).await.is_empty());
        assert!(problems(&library).await.is_empty());
        assert!(media_files(&library).await.is_empty());
        library.sources().show = Some(severance());
        library.sync_show().await.unwrap();
        assert_ne!(show(&library).await.id, before.id);
    }

    #[tokio::test]
    async fn a_show_stores_the_fields_and_actors_of_its_tvshow_nfo() {
        let library = ShowLibrary::new().await;
        with_show_nfo(&library, Some(show_nfo()), "2026-10-29T11:00:00Z");

        library.sync_show().await.unwrap();

        let show = show(&library).await;
        assert_eq!(show.title, "Severance");
        assert_eq!(show.sort_title.as_deref(), Some("severance"));
        assert_eq!(
            show.overview.as_deref(),
            Some("Mark leads a team whose memories are surgically divided.")
        );
        assert_eq!(show.certification.as_deref(), Some("TV-MA"));
        assert_eq!(show.premiered, Some(date(2022, 2, 18)));
        assert_eq!(show.end_date, None);
        assert_eq!(show.status.as_deref(), Some("Continuing"));
        assert_eq!(show.network.as_deref(), Some("Apple TV+"));
        assert_eq!(show.genres, ["Drama", "Thriller"]);
        assert_eq!(show.rating, Some(8.7));
        assert_eq!(show.nfo_modified_at, Some(at("2026-10-29T11:00:00Z")));
        assert_eq!(
            actors(&library).await,
            [
                (
                    "Adam Scott".to_owned(),
                    Some("Mark Scout".to_owned()),
                    Some(0)
                ),
                (
                    "Britt Lower".to_owned(),
                    Some("Helly R.".to_owned()),
                    Some(1)
                ),
                (
                    "Zach Cherry".to_owned(),
                    Some("Dylan George".to_owned()),
                    Some(2)
                ),
            ]
        );
        assert!(problems(&library).await.is_empty());
    }

    #[tokio::test]
    async fn a_new_show_without_a_tvshow_nfo_takes_sonarrs_fields_until_a_later_sync_finds_one() {
        let library = ShowLibrary::new().await;
        library.sync_show().await.unwrap();
        let without_nfo = show(&library).await;
        let actors_without_nfo = actors(&library).await;

        with_show_nfo(&library, Some(show_nfo()), "2026-10-29T11:00:00Z");
        library.sync_show().await.unwrap();

        assert_eq!(without_nfo.network, None);
        assert_eq!(without_nfo.sort_title, None);
        assert!(actors_without_nfo.is_empty());
        assert!(problems(&library).await.is_empty());
        assert_eq!(show(&library).await.network.as_deref(), Some("Apple TV+"));
        assert_eq!(actors(&library).await.len(), 3);
    }

    #[tokio::test]
    async fn unreadable_nfos_store_sonarrs_fields_and_one_problem_for_the_show_nfo_and_one_per_media_file()
     {
        let library = ShowLibrary::new().await;
        with_show_nfo(&library, None, "2026-10-29T11:00:00Z");
        with_episode_nfo(&library, S01E02_E03_FILE, None, "2026-10-29T11:00:00Z");

        library.sync_show().await.unwrap();

        let show = show(&library).await;
        assert_eq!(show.sort_title, None);
        assert_eq!(show.genres, ["Drama", "Mystery", "Science Fiction"]);
        let half_loop = episode(&library, 1, 2).await;
        assert_eq!(half_loop.title, "Half Loop");
        assert_eq!(
            half_loop.overview.as_deref(),
            Some("Mark takes Helly on a tour of the office.")
        );
        assert_eq!(half_loop.rating, None);
        let mut problems = problems(&library)
            .await
            .into_iter()
            .map(|x| (x.kind, x.show_id, x.media_file_id, x.path))
            .collect::<Vec<_>>();
        problems.sort_by(|a, b| a.3.cmp(&b.3));
        assert_eq!(
            problems,
            [
                (
                    SyncProblemKind::UnreadableNfo,
                    None,
                    half_loop.media_file_id,
                    nfo_path(S01E02_E03_FILE)
                ),
                (
                    SyncProblemKind::UnreadableNfo,
                    Some(show.id),
                    None,
                    SHOW_NFO.to_owned()
                ),
            ]
        );
        assert_eq!(library.sources().episode_nfo_reads, 1);
    }

    #[tokio::test]
    async fn an_nfo_with_two_episodes_fills_each_of_them() {
        let library = ShowLibrary::new().await;
        with_episode_nfo(
            &library,
            S01E02_E03_FILE,
            Some(vec![
                episode_nfo(Some(1012), 1, 2, "Half Loop", 8.1),
                episode_nfo(Some(1013), 1, 3, "In Perpetuity", 8.3),
            ]),
            "2026-10-29T11:00:00Z",
        );

        library.sync_show().await.unwrap();

        assert_eq!(
            titles_and_ratings(&library, 1).await,
            [
                (1, "Good News About Hell".to_owned(), None),
                (2, "Half Loop".to_owned(), Some(8.1)),
                (3, "In Perpetuity".to_owned(), Some(8.3)),
            ]
        );
        assert_eq!(
            episode(&library, 1, 3).await.overview.as_deref(),
            Some("The NFO's overview of In Perpetuity")
        );
        assert_eq!(
            episode(&library, 1, 3).await.air_date,
            Some(date(2022, 2, 25))
        );
    }

    #[tokio::test]
    async fn an_episode_nfo_matches_by_sonarr_id_then_by_season_and_episode_number_and_keeps_sonarrs_numbers()
     {
        let library = ShowLibrary::new().await;
        with_episode_nfo(
            &library,
            S02E01_FILE,
            Some(vec![episode_nfo(Some(1021), 3, 9, "Hello, Ms. Cobel", 8.6)]),
            "2026-10-29T11:00:00Z",
        );
        with_episode_nfo(
            &library,
            SPECIAL_FILE,
            Some(vec![episode_nfo(None, 0, 1, "Inside the Episode", 7.2)]),
            "2026-10-29T11:00:00Z",
        );

        library.sync_show().await.unwrap();

        assert_eq!(
            titles_and_ratings(&library, 2).await,
            [
                (1, "Hello, Ms. Cobel".to_owned(), Some(8.6)),
                (2, "Goodbye, Mrs. Selvig".to_owned(), None),
            ]
        );
        assert_eq!(
            titles_and_ratings(&library, 0).await,
            [(1, "Inside the Episode".to_owned(), Some(7.2))]
        );
    }

    #[tokio::test]
    async fn an_unchanged_episode_nfo_takes_sonarrs_new_episode_number() {
        let library = ShowLibrary::new().await;
        with_episode_nfo(
            &library,
            S01E01_FILE,
            Some(vec![episode_nfo(
                Some(1011),
                1,
                1,
                "Good News About Hell",
                8.0,
            )]),
            "2026-10-29T11:00:00Z",
        );
        library.sync_show().await.unwrap();

        with_source_episodes(&library, |episodes| {
            source_episode(episodes, 1, 1).episode_number = 4;
        });
        library.sync_show().await.unwrap();

        assert_eq!(
            titles_and_ratings(&library, 1).await,
            [
                (2, "Half Loop".to_owned(), None),
                (3, "In Perpetuity".to_owned(), None),
                (4, "Good News About Hell".to_owned(), Some(8.0)),
            ]
        );
        assert_eq!(library.sources().episode_nfo_reads, 1);
    }

    #[tokio::test]
    async fn show_artwork_season_posters_for_synced_seasons_and_episode_thumbs_are_stored_with_their_owners()
     {
        let library = ShowLibrary::new().await;
        {
            let mut sources = library.sources();
            sources.folder.poster =
                found(&format!("{SHOW_FOLDER}/poster.jpg"), "2026-10-28T12:30:00Z");
            sources.folder.fanart =
                found(&format!("{SHOW_FOLDER}/fanart.jpg"), "2026-10-28T12:30:00Z");
            sources.folder.clearlogo = found(
                &format!("{SHOW_FOLDER}/clearlogo.png"),
                "2026-10-28T12:30:00Z",
            );
            sources.folder.season_posters = [
                (0, "season-specials-poster.jpg"),
                (1, "season01-poster.jpg"),
                (4, "season04-poster.jpg"),
            ]
            .into_iter()
            .map(|(number, name)| {
                (
                    number,
                    found(&format!("{SHOW_FOLDER}/{name}"), "2026-10-28T12:31:00Z").unwrap(),
                )
            })
            .collect();
            for media_file in [S01E01_FILE, S01E02_E03_FILE] {
                sources
                    .episode_folders
                    .entry(media_file.to_owned())
                    .or_default()
                    .thumb = found(
                    &media_file.replace(".mkv", "-thumb.jpg"),
                    "2026-10-28T12:32:00Z",
                );
            }
        }

        library.sync_show().await.unwrap();

        let show_id = show(&library).await.id;
        let season_numbers = season_numbers(&library).await;
        let episode_numbers = episodes(&library)
            .await
            .into_iter()
            .map(|(season, x)| (x.id, (season, x.number)))
            .collect::<HashMap<_, _>>();
        let artwork = artwork(&library)
            .await
            .into_iter()
            .map(|x| {
                assert_eq!(x.show_id, Some(show_id));
                (
                    x.kind,
                    x.path.strip_prefix(SHOW_FOLDER).unwrap().to_owned(),
                    x.season_id.map(|id| season_numbers[&id]),
                    x.episode_id.map(|id| episode_numbers[&id]),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            artwork,
            [
                (
                    ArtworkKind::Thumb,
                    "/Season 01/Severance - S01E01 - Good News About Hell WEBDL-1080p-thumb.jpg"
                        .to_owned(),
                    None,
                    Some((1, 1))
                ),
                (
                    ArtworkKind::Thumb,
                    "/Season 01/Severance - S01E02-E03 - Half Loop + In Perpetuity WEBDL-1080p-thumb.jpg"
                        .to_owned(),
                    None,
                    Some((1, 2))
                ),
                (
                    ArtworkKind::Thumb,
                    "/Season 01/Severance - S01E02-E03 - Half Loop + In Perpetuity WEBDL-1080p-thumb.jpg"
                        .to_owned(),
                    None,
                    Some((1, 3))
                ),
                (ArtworkKind::Clearlogo, "/clearlogo.png".to_owned(), None, None),
                (ArtworkKind::Fanart, "/fanart.jpg".to_owned(), None, None),
                (ArtworkKind::Poster, "/poster.jpg".to_owned(), None, None),
                (
                    ArtworkKind::Poster,
                    "/season-specials-poster.jpg".to_owned(),
                    Some(0),
                    None
                ),
                (
                    ArtworkKind::Poster,
                    "/season01-poster.jpg".to_owned(),
                    Some(1),
                    None
                ),
            ]
        );
    }

    #[tokio::test]
    async fn a_season_poster_is_not_kept_for_a_season_sonarr_no_longer_lists() {
        let library = ShowLibrary::new().await;
        library.sources().folder.season_posters = vec![(
            2,
            found(
                &format!("{SHOW_FOLDER}/season02-poster.jpg"),
                "2026-10-28T12:31:00Z",
            )
            .unwrap(),
        )];
        library.sync_show().await.unwrap();
        let kept_while_listed = artwork(&library).await.len();

        with_source_episodes(&library, |episodes| {
            episodes.retain(|x| x.season_number != 2)
        });
        library.sync_show().await.unwrap();

        assert_eq!(kept_while_listed, 1);
        assert!(artwork(&library).await.is_empty());
    }

    #[tokio::test]
    async fn a_later_sync_picks_up_changed_nfos_and_images() {
        let library = ShowLibrary::new().await;
        with_show_nfo(&library, Some(show_nfo()), "2026-10-29T11:00:00Z");
        with_episode_nfo(
            &library,
            S01E01_FILE,
            Some(vec![episode_nfo(
                Some(1011),
                1,
                1,
                "Good News About Hell",
                8.0,
            )]),
            "2026-10-29T11:00:00Z",
        );
        {
            let mut sources = library.sources();
            sources.folder.poster =
                found(&format!("{SHOW_FOLDER}/poster.jpg"), "2026-10-28T12:30:00Z");
            sources.folder.fanart =
                found(&format!("{SHOW_FOLDER}/fanart.jpg"), "2026-10-28T12:30:00Z");
        }
        library.sync_show().await.unwrap();

        let ended = ShowNfo {
            status: Some("Ended".to_owned()),
            ..show_nfo()
        };
        with_show_nfo(&library, Some(ended), "2026-10-30T08:00:00Z");
        with_episode_nfo(
            &library,
            S01E01_FILE,
            Some(vec![episode_nfo(
                Some(1011),
                1,
                1,
                "Good News About Hell",
                8.2,
            )]),
            "2026-10-30T08:00:00Z",
        );
        {
            let mut sources = library.sources();
            sources.folder.poster =
                found(&format!("{SHOW_FOLDER}/poster.jpg"), "2026-10-30T08:00:00Z");
            sources.folder.fanart = None;
            sources.folder.season_posters = vec![(
                1,
                found(
                    &format!("{SHOW_FOLDER}/season01-poster.jpg"),
                    "2026-10-30T08:00:00Z",
                )
                .unwrap(),
            )];
            sources.episode_folders.get_mut(S01E01_FILE).unwrap().thumb = found(
                &S01E01_FILE.replace(".mkv", "-thumb.jpg"),
                "2026-10-30T08:00:00Z",
            );
        }
        library.sync_show().await.unwrap();

        assert_eq!(show(&library).await.status.as_deref(), Some("Ended"));
        assert_eq!(actors(&library).await.len(), 3);
        assert_eq!(episode(&library, 1, 1).await.rating, Some(8.2));
        assert_eq!(
            artwork(&library)
                .await
                .into_iter()
                .map(|x| (x.kind, x.path, x.modified_at))
                .collect::<Vec<_>>(),
            [
                (
                    ArtworkKind::Thumb,
                    S01E01_FILE.replace(".mkv", "-thumb.jpg"),
                    at("2026-10-30T08:00:00Z")
                ),
                (
                    ArtworkKind::Poster,
                    format!("{SHOW_FOLDER}/poster.jpg"),
                    at("2026-10-30T08:00:00Z")
                ),
                (
                    ArtworkKind::Poster,
                    format!("{SHOW_FOLDER}/season01-poster.jpg"),
                    at("2026-10-30T08:00:00Z")
                ),
            ]
        );
        assert_eq!(library.sources().nfo_reads, 2);
    }
}
