use crate::Title;
use crate::row_kind::ROW_SIZE;
use std::cmp::Reverse;
use uuid::Uuid;

pub fn recently_added(titles: &[Title]) -> Vec<Uuid> {
    let mut added: Vec<&Title> = titles.iter().filter(|x| x.added_at.is_some()).collect();
    added.sort_by(|a, b| {
        (Reverse(a.added_at), &a.sort_key, a.id).cmp(&(Reverse(b.added_at), &b.sort_key, b.id))
    });
    added.into_iter().take(ROW_SIZE).map(|x| x.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::title;

    fn added(name: &str, added_at: Option<&str>) -> Title {
        Title {
            added_at: added_at.map(|x| x.parse().unwrap()),
            ..title(name, &[])
        }
    }

    #[test]
    fn recently_added_lists_the_newest_added_titles_first_and_leaves_out_undated_ones() {
        let titles = [
            added("Dune", Some("2026-10-20T08:00:00Z")),
            added("Severance", Some("2026-10-25T08:00:00Z")),
            added("Arrival", Some("2026-10-29T08:00:00Z")),
            added("Andor", Some("2026-10-25T08:00:00Z")),
            added("Heat", None),
        ];

        let found = recently_added(&titles);

        assert_eq!(
            found,
            [titles[2].id, titles[3].id, titles[1].id, titles[0].id]
        );
    }

    #[test]
    fn recently_added_holds_up_to_20_titles() {
        let titles: Vec<_> = (0..21)
            .map(|x| added(&format!("Movie {x:02}"), Some("2026-10-20T08:00:00Z")))
            .collect();

        let found = recently_added(&titles);

        assert_eq!(found, titles[..20].iter().map(|x| x.id).collect::<Vec<_>>());
    }
}
