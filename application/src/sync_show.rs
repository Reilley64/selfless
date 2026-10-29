use crate::DbContext;
use crate::ports::GetSonarrShow;
use crate::ports::SonarrShow;
use domain::Episode;
use domain::MediaFile;
use domain::Season;
use domain::Show;
use std::collections::HashMap;
use std::collections::HashSet;
use tracing::Instrument;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub get_sonarr_show: GetSonarrShow,
}

#[ohno::error]
#[display("failed to sync a show from Sonarr")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>, sonarr_id: i64) -> Result<(), Error> {
    let Dependencies {
        mut db_context,
        get_sonarr_show,
    } = dependencies.into();
    async {
        let sonarr_show = get_sonarr_show
            .call(sonarr_id)
            .await
            .map_err(Error::caused_by)?;
        let mut found = HashSet::new();
        let paths = sonarr_show
            .iter()
            .flat_map(|x| &x.episodes)
            .filter_map(|x| x.episode_file.as_ref())
            .map(|x| x.path.clone())
            .collect::<HashSet<_>>();
        for path in paths {
            if matches!(tokio::fs::try_exists(&path).await, Ok(true)) {
                found.insert(path);
            }
        }
        let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
        let db = transaction.executor_mut();
        match sonarr_show {
            Some(sonarr_show) => sync(db, sonarr_id, sonarr_show, &found).await?,
            None => {
                if let Some(show) = Show::filter_by_sonarr_id(sonarr_id)
                    .first()
                    .exec(db)
                    .await
                    .map_err(Error::caused_by)?
                {
                    delete(db, show).await?;
                }
            }
        }
        transaction.commit().await.map_err(Error::caused_by)
    }
    .instrument(tracing::info_span!(
        "sync_show",
        otel.name = "sync sonarr show"
    ))
    .await
}

async fn sync(
    db: &mut dyn toasty::Executor,
    sonarr_id: i64,
    sonarr_show: SonarrShow,
    found: &HashSet<String>,
) -> Result<(), Error> {
    let show = match Show::filter_by_tvdb_id(sonarr_show.tvdb_id)
        .first()
        .exec(db)
        .await
        .map_err(Error::caused_by)?
    {
        Some(mut show) => {
            show.update()
                .sonarr_id(sonarr_id)
                .title(sonarr_show.title)
                .year(sonarr_show.year)
                .genres(sonarr_show.genres)
                .overview(sonarr_show.overview)
                .exec(db)
                .await
                .map_err(Error::caused_by)?;
            show
        }
        None => toasty::create!(Show {
            tvdb_id: sonarr_show.tvdb_id,
            sonarr_id,
            title: sonarr_show.title,
            year: sonarr_show.year,
            genres: sonarr_show.genres,
            overview: sonarr_show.overview,
        })
        .exec(db)
        .await
        .map_err(Error::caused_by)?,
    };
    let mut seasons = show
        .seasons()
        .exec(db)
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| (x.number, x))
        .collect::<HashMap<_, _>>();
    let mut episodes = episodes_of(db, &seasons.values().map(|x| x.id).collect::<Vec<_>>())
        .await?
        .into_iter()
        .map(|x| (x.tvdb_id, x))
        .collect::<HashMap<_, _>>();
    let old_media_files = episodes
        .values()
        .filter_map(|x| x.media_file_id)
        .collect::<HashSet<_>>();
    let mut synced_seasons = HashSet::new();
    let mut media_files = HashSet::new();
    for sonarr_episode in sonarr_show.episodes {
        let season_id = match seasons.get(&sonarr_episode.season_number) {
            Some(season) => season.id,
            None => {
                let season = toasty::create!(Season {
                    show_id: show.id,
                    number: sonarr_episode.season_number,
                })
                .exec(db)
                .await
                .map_err(Error::caused_by)?;
                let id = season.id;
                seasons.insert(sonarr_episode.season_number, season);
                id
            }
        };
        synced_seasons.insert(sonarr_episode.season_number);
        let (media_file_id, visible) = match &sonarr_episode.episode_file {
            Some(file) => {
                let media_file = match MediaFile::filter_by_path(&file.path)
                    .first()
                    .exec(db)
                    .await
                    .map_err(Error::caused_by)?
                {
                    Some(media_file) => media_file,
                    None => toasty::create!(MediaFile {
                        path: file.path.clone()
                    })
                    .exec(db)
                    .await
                    .map_err(Error::caused_by)?,
                };
                media_files.insert(media_file.id);
                (Some(media_file.id), found.contains(&file.path))
            }
            None => (None, false),
        };
        let date_added = sonarr_episode.episode_file.map(|x| x.date_added);
        match episodes.remove(&sonarr_episode.tvdb_id) {
            Some(mut episode) => {
                let added_at = episode.added_at.or(date_added);
                episode
                    .update()
                    .season_id(season_id)
                    .number(sonarr_episode.episode_number)
                    .title(sonarr_episode.title)
                    .air_date(sonarr_episode.air_date)
                    .overview(sonarr_episode.overview)
                    .added_at(added_at)
                    .media_file_id(media_file_id)
                    .visible(visible)
                    .exec(db)
                    .await
                    .map_err(Error::caused_by)?;
            }
            None => {
                toasty::create!(Episode {
                    season_id,
                    tvdb_id: sonarr_episode.tvdb_id,
                    number: sonarr_episode.episode_number,
                    title: sonarr_episode.title,
                    air_date: sonarr_episode.air_date,
                    overview: sonarr_episode.overview,
                    added_at: date_added,
                    media_file_id,
                    visible,
                })
                .exec(db)
                .await
                .map_err(Error::caused_by)?;
            }
        }
    }
    delete_episodes(db, episodes.into_values().map(|x| x.id).collect()).await?;
    delete_seasons(
        db,
        seasons
            .into_values()
            .filter(|x| !synced_seasons.contains(&x.number))
            .map(|x| x.id)
            .collect(),
    )
    .await?;
    delete_media_files(
        db,
        old_media_files.difference(&media_files).copied().collect(),
    )
    .await
}

async fn delete(db: &mut dyn toasty::Executor, show: Show) -> Result<(), Error> {
    let seasons = show
        .seasons()
        .exec(db)
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| x.id)
        .collect::<Vec<_>>();
    let media_files = episodes_of(db, &seasons)
        .await?
        .into_iter()
        .filter_map(|x| x.media_file_id)
        .collect::<HashSet<_>>();
    show.delete().exec(db).await.map_err(Error::caused_by)?;
    delete_media_files(db, media_files.into_iter().collect()).await
}

async fn episodes_of(
    db: &mut dyn toasty::Executor,
    season_ids: &[Uuid],
) -> Result<Vec<Episode>, Error> {
    if season_ids.is_empty() {
        return Ok(Vec::new());
    }
    Episode::filter(Episode::fields().season_id().in_list(season_ids))
        .exec(db)
        .await
        .map_err(Error::caused_by)
}

async fn delete_episodes(db: &mut dyn toasty::Executor, ids: Vec<Uuid>) -> Result<(), Error> {
    if ids.is_empty() {
        return Ok(());
    }
    Episode::filter(Episode::fields().id().in_list(ids))
        .delete()
        .exec(db)
        .await
        .map_err(Error::caused_by)
}

async fn delete_seasons(db: &mut dyn toasty::Executor, ids: Vec<Uuid>) -> Result<(), Error> {
    if ids.is_empty() {
        return Ok(());
    }
    Season::filter(Season::fields().id().in_list(ids))
        .delete()
        .exec(db)
        .await
        .map_err(Error::caused_by)
}

async fn delete_media_files(db: &mut dyn toasty::Executor, ids: Vec<Uuid>) -> Result<(), Error> {
    if ids.is_empty() {
        return Ok(());
    }
    MediaFile::filter(MediaFile::fields().id().in_list(ids))
        .delete()
        .exec(db)
        .await
        .map_err(Error::caused_by)
}
