use crate::Title;
use crate::genre_counts;
use crate::row_kind::ROW_SIZE;
use std::cmp::Reverse;
use uuid::Uuid;

const GENRE_ROWS: usize = 6;

pub fn genre_rows(titles: &[Title]) -> Vec<(String, Vec<Uuid>)> {
    let mut genres: Vec<_> = genre_counts(titles.iter().map(|x| x.genres.as_slice()))
        .into_iter()
        .collect();
    genres.sort_by(|a, b| (Reverse(a.1), a.0).cmp(&(Reverse(b.1), b.0)));
    let mut by_sort_title: Vec<&Title> = titles.iter().collect();
    by_sort_title.sort_by(|a, b| (&a.sort_key, a.id).cmp(&(&b.sort_key, b.id)));
    genres
        .into_iter()
        .take(GENRE_ROWS)
        .map(|(genre, _)| {
            let ids = by_sort_title
                .iter()
                .filter(|x| x.genres.iter().any(|x| x == genre))
                .take(ROW_SIZE)
                .map(|x| x.id)
                .collect();
            (genre.to_owned(), ids)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sort_key;
    use crate::test::title;

    #[test]
    fn the_genre_rows_are_the_6_genres_with_the_most_titles_each_sorted_by_sort_title() {
        let departed = Title {
            sort_key: sort_key(Some("Departed"), "The Departed"),
            ..title("The Departed", &["Drama", "Crime", "Thriller"])
        };
        let titles = [
            title("Arrival", &["Drama", "Mystery"]),
            departed,
            title("Zodiac", &["Crime", "Mystery", "Thriller"]),
            title("Airplane!", &["Comedy"]),
            title("Heat", &["Action"]),
            title("Severance", &["Drama", "Comedy", "Horror"]),
            title("Barry", &["Drama", "Comedy", "Crime", "Horror"]),
        ];
        let name = |id: &Uuid| {
            titles
                .iter()
                .find(|x| x.id == *id)
                .unwrap()
                .sort_key
                .1
                .clone()
        };

        let rows: Vec<_> = genre_rows(&titles)
            .into_iter()
            .map(|(genre, ids)| (genre, ids.iter().map(name).collect::<Vec<_>>()))
            .collect();

        let row = |genre: &str, names: &[&str]| {
            (
                genre.to_owned(),
                names.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            rows,
            [
                row("Drama", &["Arrival", "Barry", "The Departed", "Severance"]),
                row("Comedy", &["Airplane!", "Barry", "Severance"]),
                row("Crime", &["Barry", "The Departed", "Zodiac"]),
                row("Horror", &["Barry", "Severance"]),
                row("Mystery", &["Arrival", "Zodiac"]),
                row("Thriller", &["The Departed", "Zodiac"]),
            ]
        );
    }

    #[test]
    fn a_genre_row_holds_up_to_20_titles() {
        let titles: Vec<_> = (0..21)
            .map(|x| title(&format!("Movie {x:02}"), &["Drama"]))
            .collect();

        let rows = genre_rows(&titles);

        assert_eq!(
            rows,
            [(
                "Drama".to_owned(),
                titles[..20].iter().map(|x| x.id).collect::<Vec<_>>()
            )]
        );
    }
}
