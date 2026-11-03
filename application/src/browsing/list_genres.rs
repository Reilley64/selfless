mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Genre;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::TitleKind;
use std::collections::BTreeMap;

pub async fn call(dependencies: impl Into<Dependencies>, kind: TitleKind) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let titles: Vec<Vec<String>> = match kind {
        TitleKind::Movie => db_context
            .movies()
            .filter(domain::Movie::in_library_filter())
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| x.genres)
            .collect(),
        TitleKind::Show => db_context
            .shows()
            .filter(domain::Show::in_library_filter())
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| x.genres)
            .collect(),
    };
    let mut counts = BTreeMap::<String, i64>::new();
    for name in titles.into_iter().flatten() {
        *counts.entry(name).or_default() += 1;
    }
    Ok(Output {
        genres: counts
            .into_iter()
            .map(|(name, count)| Genre { name, count })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::persistence::DbContext;
    use crate::test::add_season;
    use crate::test::episode;
    use crate::test::mock_db_context;
    use crate::test::movie;
    use crate::test::show;

    fn genres(names: &[&str]) -> Vec<String> {
        names.iter().map(|x| (*x).to_owned()).collect()
    }

    async fn add_show(db_context: &mut DbContext, title: &str, names: &[&str], in_library: bool) {
        let show = show(db_context, title)
            .genres(genres(names))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let season_id = add_season(db_context, show.id, 1).await;
        episode(db_context, season_id)
            .in_library(in_library)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }

    async fn list(db_context: &DbContext, kind: TitleKind) -> Vec<(String, i64)> {
        call(
            Dependencies {
                db_context: db_context.clone(),
            },
            kind,
        )
        .await
        .unwrap()
        .genres
        .into_iter()
        .map(|x| (x.name, x.count))
        .collect()
    }

    fn counts(expected: &[(&str, i64)]) -> Vec<(String, i64)> {
        expected
            .iter()
            .map(|(name, count)| ((*name).to_owned(), *count))
            .collect()
    }

    #[tokio::test]
    async fn movie_genres_count_the_movies_in_the_library_of_each_genre_by_name() {
        let mut db_context = mock_db_context().await;
        for (title, names, in_library) in [
            ("Sicario", &["Crime", "Drama"][..], true),
            ("Arrival", &["Science Fiction", "Drama"][..], true),
            ("Gone", &["Comedy", "Drama"][..], false),
        ] {
            movie(&db_context, title)
                .genres(genres(names))
                .in_library(in_library)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        add_show(&mut db_context, "Severance", &["Mystery"], true).await;

        assert_eq!(
            list(&db_context, TitleKind::Movie).await,
            counts(&[("Crime", 1), ("Drama", 2), ("Science Fiction", 1)])
        );
    }

    #[tokio::test]
    async fn show_genres_count_the_shows_with_an_episode_in_the_library() {
        let mut db_context = mock_db_context().await;
        add_show(&mut db_context, "Severance", &["Mystery", "Drama"], true).await;
        add_show(
            &mut db_context,
            "Andor",
            &["Drama", "Science Fiction"],
            true,
        )
        .await;
        add_show(&mut db_context, "Gone", &["Comedy"], false).await;
        movie(&db_context, "Sicario")
            .genres(genres(&["Crime"]))
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        assert_eq!(
            list(&db_context, TitleKind::Show).await,
            counts(&[("Drama", 2), ("Mystery", 1), ("Science Fiction", 1)])
        );
    }
}
