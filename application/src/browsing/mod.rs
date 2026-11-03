mod image;
mod more_like_this;
mod resizer;
mod tile;

pub mod clean_image_cache;
pub mod get_home;
pub mod get_image;
pub mod get_movie;
pub mod get_show;
pub mod list_genres;
pub mod list_movies;
pub mod list_season_episodes;
pub mod list_shows;

pub use image::Image;
pub use image::ImageUrl;
pub use image::TileImages;
use image::title_images;
pub use resizer::Resizer;

use domain::Episode;
use domain::Season;
use domain::Show;
use more_like_this::more_like_this;
use std::collections::HashMap;
use uuid::Uuid;

pub use tile::Cursor;
pub use tile::SortKey;
pub use tile::Tile;
pub use tile::TileKind;

const PAGE_SIZE: usize = 60;

fn visible_show() -> toasty::stmt::Expr<bool> {
    Show::fields().seasons().any(visible_season())
}

fn visible_season() -> toasty::stmt::Expr<bool> {
    Season::fields()
        .episodes()
        .any(Episode::fields().visible().eq(true))
}

fn season_counts(seasons: &[Season]) -> HashMap<Uuid, i64> {
    let mut counts = HashMap::new();
    for season in seasons.iter().filter(|x| x.number > 0) {
        *counts.entry(season.show_id).or_default() += 1;
    }
    counts
}

fn page<T>(mut keyed: Vec<(Cursor, T)>, after: Option<&Cursor>) -> (Vec<T>, Option<Cursor>) {
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.retain(|(key, _)| after.is_none_or(|after| key > after));
    let next = (keyed.len() > PAGE_SIZE).then(|| keyed[PAGE_SIZE - 1].0.clone());
    keyed.truncate(PAGE_SIZE);
    (keyed.into_iter().map(|(_, x)| x).collect(), next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::Timestamp;
    use jiff::ToSpan;
    use jiff::civil::date;
    use std::cmp::Reverse;
    use uuid::Uuid;

    fn title(sort_title: &str, id: u128) -> (Cursor, u128) {
        (
            Cursor {
                key: SortKey::Title(sort_title.to_owned()),
                id: Uuid::from_u128(id),
            },
            id,
        )
    }

    #[test]
    fn a_page_holds_60_titles_in_sort_title_order_and_a_cursor_for_the_rest() {
        let titles = || (0..61).rev().map(|x| title(&format!("movie {x:02}"), x));

        let (first, next) = page(titles().collect(), None);
        let (second, last) = page(titles().collect(), next.as_ref());

        assert_eq!(first, (0..60).collect::<Vec<_>>());
        assert_eq!(second, [60]);
        assert_eq!(last, None);
    }

    #[test]
    fn titles_with_the_same_sort_title_split_across_pages_by_id() {
        let titles = || {
            (0..59)
                .map(|x| title(&format!("movie {x:02}"), x))
                .chain([title("movie 59", 200), title("movie 59", 100)])
        };

        let (first, next) = page(titles().collect(), None);
        let (second, _) = page(titles().collect(), next.as_ref());

        assert_eq!(first[59], 100);
        assert_eq!(second, [200]);
    }

    #[test]
    fn undated_titles_and_ties_split_across_pages_by_id_under_the_date_sorts() {
        let keys: [fn(Option<i64>) -> SortKey; 2] = [
            |days| {
                SortKey::Added(Reverse(
                    days.map(|x| Timestamp::from_second(x * 86_400).unwrap()),
                ))
            },
            |days| SortKey::Released(Reverse(days.map(|x| date(2000, 1, 1) + x.days()))),
        ];
        for key in keys {
            let titles = || {
                let cursor = |days, id| {
                    (
                        Cursor {
                            key: key(days),
                            id: Uuid::from_u128(id),
                        },
                        id,
                    )
                };
                (0..58)
                    .map(|x| cursor(Some(100 - x as i64), x))
                    .chain([cursor(None, 300), cursor(None, 100), cursor(None, 200)])
                    .collect::<Vec<_>>()
            };

            let (first, next) = page(titles(), None);
            let (second, last) = page(titles(), next.as_ref());

            assert_eq!(first[..58], (0..58).collect::<Vec<_>>());
            assert_eq!(first[58..], [100, 200]);
            assert_eq!(second, [300]);
            assert_eq!(last, None);
        }
    }
}
