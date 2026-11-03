mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Artwork;
pub use output::Episode;
pub use output::Output;

use crate::ports::persistence::DbScope;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, season_id: Uuid) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let episodes = db_context
        .episodes()
        .filter(
            domain::Episode::fields()
                .season_id()
                .eq(season_id)
                .and(domain::Episode::in_library_filter()),
        )
        .order_by(domain::Episode::fields().number().asc())
        .include(domain::Episode::fields().artwork())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|episode| {
            let mut artwork = episode.artwork.into_inner();
            artwork.sort_by_cached_key(domain::Artwork::listing_order);
            Episode {
                id: episode.id,
                number: episode.number,
                title: episode.title,
                air_date: episode.air_date,
                overview: episode.overview,
                added_at: episode.added_at,
                media_file_id: episode.media_file_id,
                rating: episode.rating,
                artwork: artwork
                    .into_iter()
                    .map(|x| Artwork {
                        kind: x.kind,
                        path: x.path,
                        modified_at: x.modified_at,
                    })
                    .collect(),
            }
        })
        .collect();
    Ok(Output { episodes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::persistence::DbContext;
    use crate::test::at;
    use crate::test::mock_db_context;
    use domain::ArtworkKind;
    use domain::SonarrId;
    use domain::TvdbId;

    async fn seed_show(db_context: &mut DbContext, tvdb_id: i64, title: &str) -> Uuid {
        db_context
            .shows()
            .create()
            .tvdb_id(TvdbId(tvdb_id))
            .sonarr_id(SonarrId(tvdb_id + 1_000))
            .title(title)
            .genres(Vec::<String>::new())
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    async fn seed_season(db_context: &mut DbContext, show_id: Uuid, number: i64) -> Uuid {
        db_context
            .seasons()
            .create()
            .show_id(show_id)
            .number(number)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    async fn seed_episode(
        db_context: &mut DbContext,
        season_id: Uuid,
        tvdb_id: i64,
        number: i64,
        in_library: bool,
    ) -> Uuid {
        db_context
            .episodes()
            .create()
            .season_id(season_id)
            .tvdb_id(TvdbId(tvdb_id))
            .number(number)
            .title(format!("Episode {number}"))
            .in_library(in_library)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    async fn seed_artwork(
        db_context: &mut DbContext,
        show_id: Uuid,
        season_id: Option<Uuid>,
        episode_id: Option<Uuid>,
        kind: ArtworkKind,
        path: &str,
    ) {
        db_context
            .artwork()
            .create()
            .show_id(Some(show_id))
            .season_id(season_id)
            .episode_id(episode_id)
            .kind(kind)
            .path(path)
            .modified_at(at("2026-10-28T12:30:00Z"))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn only_episodes_in_the_library_are_listed_by_number() {
        let mut db_context = mock_db_context().await;
        let show_id = seed_show(&mut db_context, 1, "Severance").await;
        let season_id = seed_season(&mut db_context, show_id, 1).await;
        seed_episode(&mut db_context, season_id, 3, 3, true).await;
        seed_episode(&mut db_context, season_id, 1, 1, false).await;
        seed_episode(&mut db_context, season_id, 2, 2, true).await;
        let other_season = seed_season(&mut db_context, show_id, 2).await;
        seed_episode(&mut db_context, other_season, 4, 1, true).await;

        let episodes = call(Dependencies { db_context }, season_id)
            .await
            .unwrap()
            .episodes;

        assert_eq!(
            episodes.iter().map(|x| x.number).collect::<Vec<_>>(),
            [2, 3]
        );
    }

    #[tokio::test]
    async fn an_episodes_artwork_is_listed() {
        let mut db_context = mock_db_context().await;
        let show_id = seed_show(&mut db_context, 1, "Severance").await;
        let season_id = seed_season(&mut db_context, show_id, 1).await;
        let episode_id = seed_episode(&mut db_context, season_id, 1, 1, true).await;
        seed_artwork(
            &mut db_context,
            show_id,
            Some(season_id),
            None,
            ArtworkKind::Poster,
            "/media/TV/Severance/season01-poster.jpg",
        )
        .await;
        seed_artwork(
            &mut db_context,
            show_id,
            None,
            Some(episode_id),
            ArtworkKind::Thumb,
            "/media/TV/Severance/Season 01/S01E01-thumb.jpg",
        )
        .await;

        let [episode] = call(Dependencies { db_context }, season_id)
            .await
            .unwrap()
            .episodes
            .try_into()
            .unwrap();

        assert_eq!(
            episode.artwork,
            [Artwork {
                kind: ArtworkKind::Thumb,
                path: "/media/TV/Severance/Season 01/S01E01-thumb.jpg".to_owned(),
                modified_at: at("2026-10-28T12:30:00Z"),
            }]
        );
    }
}
