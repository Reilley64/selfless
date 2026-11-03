mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Actor;
pub use output::Artwork;
pub use output::Output;
pub use output::Show;

use crate::ports::persistence::DbScope;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let mut shows = db_context
        .shows()
        .filter(domain::Show::in_library_filter())
        .include(domain::Show::fields().actors())
        .include(
            domain::Show::fields().artwork().filter(
                domain::Artwork::fields()
                    .season_id()
                    .is_none()
                    .and(domain::Artwork::fields().episode_id().is_none()),
            ),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|show| Show {
            id: show.id,
            title: show.title,
            sort_title: show.sort_title,
            year: show.year,
            genres: show.genres,
            overview: show.overview,
            certification: show.certification,
            premiered: show.premiered,
            end_date: show.end_date,
            status: show.status,
            network: show.network,
            rating: show.rating,
            actors: actors(show.actors.into_inner()),
            artwork: artwork(show.artwork.into_inner()),
        })
        .collect::<Vec<_>>();
    shows.sort_by_cached_key(|x| domain::sort_key(x.sort_title.as_deref(), &x.title));
    Ok(Output { shows })
}

fn actors(stored: Vec<domain::Actor>) -> Vec<Actor> {
    let mut actors = stored
        .into_iter()
        .map(|x| Actor {
            name: x.name,
            role: x.role,
            order: x.order,
        })
        .collect::<Vec<_>>();
    actors.sort_by_key(|x| x.order);
    actors
}

fn artwork(stored: Vec<domain::Artwork>) -> Vec<Artwork> {
    let mut artwork = stored
        .into_iter()
        .map(|x| Artwork {
            kind: x.kind,
            path: x.path,
            modified_at: x.modified_at,
        })
        .collect::<Vec<_>>();
    artwork.sort_by(|a, b| a.path.cmp(&b.path));
    artwork
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::persistence::DbContext;
    use crate::test::at;
    use crate::test::mock_db_context;
    use domain::ArtworkKind;
    use uuid::Uuid;

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

    async fn seed_show_in_library(db_context: &mut DbContext, tvdb_id: i64, title: &str) -> Uuid {
        let show_id = seed_show(db_context, tvdb_id, title).await;
        let season_id = seed_season(db_context, show_id, 1).await;
        seed_episode(db_context, season_id, tvdb_id, 1, true).await;
        show_id
    }

    fn titles(shows: &[Show]) -> Vec<&str> {
        shows.iter().map(|x| x.title.as_str()).collect()
    }

    #[tokio::test]
    async fn only_shows_with_an_episode_in_the_library_are_listed() {
        let mut db_context = mock_db_context().await;
        seed_show_in_library(&mut db_context, 1, "Severance").await;
        let hidden = seed_show(&mut db_context, 2, "Andor").await;
        let hidden_season = seed_season(&mut db_context, hidden, 1).await;
        seed_episode(&mut db_context, hidden_season, 2, 1, false).await;
        seed_show(&mut db_context, 3, "Silo").await;

        let shows = call(Dependencies { db_context }).await.unwrap().shows;

        assert_eq!(titles(&shows), ["Severance"]);
    }

    #[tokio::test]
    async fn shows_are_listed_by_their_sort_title() {
        let mut db_context = mock_db_context().await;
        seed_show_in_library(&mut db_context, 1, "Severance").await;
        let andor = seed_show_in_library(&mut db_context, 2, "Andor").await;
        let mut andor = db_context
            .shows()
            .find(andor)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap();
        andor
            .update()
            .sort_title(Some("Star Wars: Andor".to_owned()))
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        let shows = call(Dependencies { db_context }).await.unwrap().shows;

        assert_eq!(titles(&shows), ["Severance", "Andor"]);
    }

    #[tokio::test]
    async fn a_shows_artwork_is_listed_without_its_season_and_episode_artwork() {
        let mut db_context = mock_db_context().await;
        let show_id = seed_show(&mut db_context, 1, "Severance").await;
        let season_id = seed_season(&mut db_context, show_id, 1).await;
        let episode_id = seed_episode(&mut db_context, season_id, 1, 1, true).await;
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
        seed_artwork(
            &mut db_context,
            show_id,
            None,
            Some(episode_id),
            ArtworkKind::Thumb,
            "/media/TV/Severance/Season 01/S01E01-thumb.jpg",
        )
        .await;

        let [show] = call(Dependencies { db_context })
            .await
            .unwrap()
            .shows
            .try_into()
            .unwrap();

        assert_eq!(
            show.artwork,
            [Artwork {
                kind: ArtworkKind::Poster,
                path: "/media/TV/Severance/poster.jpg".to_owned(),
                modified_at: at("2026-10-28T12:30:00Z"),
            }]
        );
    }
}
