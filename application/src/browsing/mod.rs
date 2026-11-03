mod image;
mod resizer;
mod tile;

pub mod clean_image_cache;
pub mod get_image;
pub mod list_movies;
pub mod list_shows;

pub use image::Image;
pub use image::ImageUrl;
pub use image::TileImages;
use image::title_images;
pub use resizer::Resizer;
pub use tile::Cursor;
pub use tile::Tile;
pub use tile::TileKind;

const PAGE_SIZE: usize = 60;

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
    use uuid::Uuid;

    fn title(sort_title: &str, id: u128) -> (Cursor, u128) {
        (
            Cursor {
                sort_title: sort_title.to_owned(),
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
}
