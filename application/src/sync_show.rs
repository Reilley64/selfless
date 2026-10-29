use crate::DbContext;
use crate::SourceHealth;
use crate::library_items;
use crate::library_items::Change;
use crate::media_files;
use crate::media_files::OnDisk;
use crate::ports::GetSonarrShow;
use crate::ports::SonarrShow;
use domain::Episode;
use domain::Season;
use domain::Show;
use domain::Source;
use std::collections::HashMap;
use std::collections::HashSet;
use tracing::Instrument;
use tracing::Span;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub get_sonarr_show: GetSonarrShow,
    pub source_health: SourceHealth,
}

#[ohno::error]
#[display("failed to sync a show from Sonarr")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>, sonarr_id: i64) -> Result<(), Error> {
    let Dependencies {
        mut db_context,
        get_sonarr_show,
        source_health,
    } = dependencies.into();
    async {
        let sonarr_show = get_sonarr_show.call(sonarr_id).await;
        match source_health
            .api_call(Source::Sonarr, sonarr_show)
            .map_err(Error::caused_by)?
        {
            Some(sonarr_show) => save(&mut db_context, sonarr_show).await,
            None => match Show::filter_by_sonarr_id(sonarr_id)
                .first()
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?
            {
                Some(show) => remove(&mut db_context, show).await,
                None => Ok(()),
            },
        }
    }
    .instrument(span())
    .await
}

pub(crate) fn span() -> Span {
    tracing::info_span!("sync_show", otel.name = "sync sonarr show")
}

pub(crate) async fn save(db_context: &mut DbContext, sonarr_show: SonarrShow) -> Result<(), Error> {
    let mut on_disk = HashMap::new();
    for file in sonarr_show
        .episodes
        .iter()
        .filter_map(|x| x.episode_file.as_ref())
    {
        if !on_disk.contains_key(&file.path) {
            on_disk.insert(file.path.clone(), media_files::on_disk(&file.path).await);
        }
    }
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let change = sync(transaction.executor_mut(), sonarr_show, &on_disk).await?;
    transaction.commit().await.map_err(Error::caused_by)?;
    if let Some(change) = change {
        library_items::count(Source::Sonarr, change);
    }
    Ok(())
}

pub(crate) async fn remove(db_context: &mut DbContext, show: Show) -> Result<(), Error> {
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    delete(transaction.executor_mut(), show).await?;
    transaction.commit().await.map_err(Error::caused_by)?;
    library_items::count(Source::Sonarr, Change::Removed);
    Ok(())
}

async fn sync(
    db: &mut dyn toasty::Executor,
    sonarr_show: SonarrShow,
    on_disk: &HashMap<String, Option<OnDisk>>,
) -> Result<Option<Change>, Error> {
    let series = sonarr_show.series;
    let (show, show_change) = match Show::filter_by_tvdb_id(series.tvdb_id)
        .first()
        .exec(db)
        .await
        .map_err(Error::caused_by)?
    {
        Some(mut show) => {
            if show.sonarr_id != series.sonarr_id
                || show.title != series.title
                || show.year != series.year
                || show.genres != series.genres
                || show.overview != series.overview
            {
                show.update()
                    .sonarr_id(series.sonarr_id)
                    .title(series.title)
                    .year(series.year)
                    .genres(series.genres)
                    .overview(series.overview)
                    .exec(db)
                    .await
                    .map_err(Error::caused_by)?;
                (show, Some(Change::Changed))
            } else {
                (show, None)
            }
        }
        None => (
            toasty::create!(Show {
                tvdb_id: series.tvdb_id,
                sonarr_id: series.sonarr_id,
                title: series.title,
                year: series.year,
                genres: series.genres,
                overview: series.overview,
            })
            .exec(db)
            .await
            .map_err(Error::caused_by)?,
            Some(Change::Added),
        ),
    };
    let mut changed = show_change.is_some();
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
                changed = true;
                let id = season.id;
                seasons.insert(sonarr_episode.season_number, season);
                id
            }
        };
        synced_seasons.insert(sonarr_episode.season_number);
        let (media_file_id, visible) = match &sonarr_episode.episode_file {
            Some(file) => {
                let file_on_disk = on_disk.get(&file.path).copied().flatten();
                let (id, file_changed) = media_files::save(db, &file.path, file_on_disk)
                    .await
                    .map_err(Error::caused_by)?;
                changed |= file_changed;
                media_files.insert(id);
                (Some(id), file_on_disk.is_some())
            }
            None => (None, false),
        };
        let date_added = sonarr_episode.episode_file.map(|x| x.date_added);
        match episodes.remove(&sonarr_episode.tvdb_id) {
            Some(mut episode) => {
                let added_at = episode.added_at.or(date_added);
                if episode.season_id == season_id
                    && episode.number == sonarr_episode.episode_number
                    && episode.title == sonarr_episode.title
                    && episode.air_date == sonarr_episode.air_date
                    && episode.overview == sonarr_episode.overview
                    && episode.added_at == added_at
                    && episode.media_file_id == media_file_id
                    && episode.visible == visible
                {
                    continue;
                }
                changed = true;
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
                changed = true;
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
    let episodes = episodes.into_values().map(|x| x.id).collect::<Vec<_>>();
    let seasons = seasons
        .into_values()
        .filter(|x| !synced_seasons.contains(&x.number))
        .map(|x| x.id)
        .collect::<Vec<_>>();
    let media_files = old_media_files
        .difference(&media_files)
        .copied()
        .collect::<Vec<_>>();
    changed |= !episodes.is_empty() || !seasons.is_empty() || !media_files.is_empty();
    delete_episodes(db, episodes).await?;
    delete_seasons(db, seasons).await?;
    delete_media_files(db, media_files).await?;
    Ok(show_change.or(changed.then_some(Change::Changed)))
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
    media_files::delete(db, ids).await.map_err(Error::caused_by)
}
