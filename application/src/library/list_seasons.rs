mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Artwork;
pub use output::Output;
pub use output::Season;

use crate::ports::persistence::DbScope;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, show_id: Uuid) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let mut seasons = db_context
        .seasons()
        .filter(
            domain::Season::fields()
                .show_id()
                .eq(show_id)
                .and(domain::Season::in_library_filter()),
        )
        .include(domain::Season::fields().artwork())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    seasons.sort_by_key(domain::Season::listing_order);
    let seasons = seasons
        .into_iter()
        .map(|x| {
            let mut artwork = x.artwork.into_inner();
            artwork.sort_by_cached_key(domain::Artwork::listing_order);
            Season {
                id: x.id,
                number: x.number,
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
    Ok(Output { seasons })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::persistence::DbContext;
    use crate::test::at;
    use crate::test::mock_db_context;
    use domain::ArtworkKind;

    async fn seed_show(db_context: &mut DbContext, tvdb_id: i64, title: &str) -> Uuid {
        db_context
            .shows()
            .create()
            .tvdb_id(tvdb_id)
            .sonarr_id(tvdb_id)
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
        visible: bool,
    ) -> Uuid {
        db_context
            .episodes()
            .create()
            .season_id(season_id)
            .tvdb_id(tvdb_id)
            .number(number)
            .title(format!("Episode {number}"))
            .visible(visible)
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
    async fn only_seasons_with_an_episode_in_the_library_are_listed_with_specials_after_the_numbered_seasons()
     {
        let mut db_context = mock_db_context().await;
        let show_id = seed_show(&mut db_context, 1, "Severance").await;
        for (number, visible) in [(0, true), (2, true), (1, true), (3, false)] {
            let season_id = seed_season(&mut db_context, show_id, number).await;
            seed_episode(&mut db_context, season_id, number, 1, visible).await;
        }
        let other_show = seed_show(&mut db_context, 2, "Andor").await;
        let other_season = seed_season(&mut db_context, other_show, 1).await;
        seed_episode(&mut db_context, other_season, 10, 1, true).await;

        let seasons = call(Dependencies { db_context }, show_id)
            .await
            .unwrap()
            .seasons;

        assert_eq!(
            seasons.iter().map(|x| x.number).collect::<Vec<_>>(),
            [1, 2, 0]
        );
    }

    #[tokio::test]
    async fn a_seasons_artwork_is_listed() {
        let mut db_context = mock_db_context().await;
        let show_id = seed_show(&mut db_context, 1, "Severance").await;
        let season_id = seed_season(&mut db_context, show_id, 1).await;
        seed_episode(&mut db_context, season_id, 1, 1, true).await;
        seed_artwork(
            &mut db_context,
            show_id,
            None,
            None,
            ArtworkKind::Poster,
            "/media/TV/Severance/poster.jpg",
        )
        .await;
        seed_artwork(
            &mut db_context,
            show_id,
            Some(season_id),
            None,
            ArtworkKind::Poster,
            "/media/TV/Severance/season01-poster.jpg",
        )
        .await;

        let [season] = call(Dependencies { db_context }, show_id)
            .await
            .unwrap()
            .seasons
            .try_into()
            .unwrap();

        assert_eq!(
            season.artwork,
            [Artwork {
                kind: ArtworkKind::Poster,
                path: "/media/TV/Severance/season01-poster.jpg".to_owned(),
                modified_at: at("2026-10-28T12:30:00Z"),
            }]
        );
    }
}
