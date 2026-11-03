use crate::Title;
use crate::row_kind::ROW_SIZE;
use std::cmp::Reverse;
use uuid::Uuid;

pub fn more_like_this(title_id: Uuid, genres: &[String], titles: &[Title]) -> Vec<Uuid> {
    let mut ranked: Vec<_> = titles
        .iter()
        .filter(|x| x.id != title_id)
        .filter_map(|x| {
            let shared = x.genres.iter().filter(|x| genres.contains(x)).count();
            (shared > 0).then_some((Reverse(shared), &x.sort_key, x.id))
        })
        .collect();
    ranked.sort();
    ranked
        .into_iter()
        .take(ROW_SIZE)
        .map(|(_, _, id)| id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sort_key;
    use crate::test::title;

    #[test]
    fn more_like_this_ranks_titles_by_shared_genres_then_sort_title_without_the_title_itself() {
        let arrival = title("Arrival", &["Drama", "Science Fiction", "Mystery"]);
        let prestige = Title {
            sort_key: sort_key(Some("Prestige"), "The Prestige"),
            ..title("The Prestige", &["Mystery"])
        };
        let titles = [
            arrival.clone(),
            title("Dune", &["Science Fiction", "Drama"]),
            title("Blade Runner 2049", &["Science Fiction"]),
            title("Sicario", &["Drama"]),
            prestige,
            title("Amélie", &["Comedy"]),
            title("Severance", &["Drama", "Mystery", "Science Fiction"]),
        ];

        let found = more_like_this(arrival.id, &arrival.genres, &titles);

        assert_eq!(found, [6, 1, 2, 4, 3].map(|x| titles[x].id),);
    }

    #[test]
    fn more_like_this_holds_up_to_20_titles() {
        let arrival = title("Arrival", &["Drama"]);
        let titles: Vec<_> = (0..21)
            .map(|x| title(&format!("Movie {x:02}"), &["Drama"]))
            .collect();

        let found = more_like_this(arrival.id, &arrival.genres, &titles);

        assert_eq!(found, titles[..20].iter().map(|x| x.id).collect::<Vec<_>>());
    }
}
