use crate::Episode;
use crate::Movie;
use crate::Show;
use crate::sort_key;
use jiff::Timestamp;
use jiff::civil::Date;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
pub struct Title {
    pub id: Uuid,
    pub genres: Vec<String>,
    pub sort_key: (String, String),
    pub added_at: Option<Timestamp>,
    pub released: Option<Date>,
}

impl Title {
    pub fn movie(movie: &Movie) -> Self {
        Self {
            id: movie.id,
            genres: movie.genres.clone(),
            sort_key: sort_key(movie.sort_title.as_deref(), &movie.title),
            added_at: movie.added_at,
            released: movie.premiered,
        }
    }

    pub fn show<'a>(show: &Show, episodes: impl IntoIterator<Item = &'a Episode>) -> Self {
        let in_library: Vec<&Episode> = episodes.into_iter().filter(|x| x.in_library).collect();
        Self {
            id: show.id,
            genres: show.genres.clone(),
            sort_key: sort_key(show.sort_title.as_deref(), &show.title),
            added_at: in_library.iter().filter_map(|x| x.added_at).max(),
            released: in_library.iter().filter_map(|x| x.air_date).max(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::episode;
    use crate::test::show;
    use jiff::civil::date;

    #[test]
    fn a_show_is_added_and_released_with_its_newest_episodes_in_the_library() {
        let dated = |number, added_at: &str, air_date, in_library| Episode {
            added_at: Some(added_at.parse().unwrap()),
            air_date: Some(air_date),
            ..episode(number, in_library)
        };
        let episodes = [
            dated(1, "2026-10-25T08:00:00Z", date(2025, 3, 21), true),
            dated(2, "2026-10-20T08:00:00Z", date(2025, 3, 28), true),
            dated(3, "2026-10-30T08:00:00Z", date(2025, 4, 4), false),
            episode(4, true),
        ];

        let title = Title::show(&show("Severance"), &episodes);

        assert_eq!(
            (title.added_at, title.released),
            (
                Some("2026-10-25T08:00:00Z".parse().unwrap()),
                Some(date(2025, 3, 28))
            )
        );
    }
}
