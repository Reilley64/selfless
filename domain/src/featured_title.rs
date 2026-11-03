use crate::Title;
use std::cmp::Reverse;
use uuid::Uuid;

pub fn featured_title(popular_now: &[Uuid], titles: &[Title]) -> Option<Uuid> {
    popular_now.first().copied().or_else(|| {
        titles
            .iter()
            .filter(|x| x.released.is_some())
            .min_by(|a, b| {
                (Reverse(a.released), &a.sort_key, a.id).cmp(&(
                    Reverse(b.released),
                    &b.sort_key,
                    b.id,
                ))
            })
            .map(|x| x.id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::title;
    use jiff::civil::Date;
    use jiff::civil::date;

    fn released(name: &str, released: Option<Date>) -> Title {
        Title {
            released,
            ..title(name, &[])
        }
    }

    #[test]
    fn the_featured_title_is_the_first_of_popular_now() {
        let titles = [released("Dune", Some(date(2021, 10, 22)))];
        let arrival = Uuid::new_v4();

        assert_eq!(featured_title(&[arrival], &titles), Some(arrival));
    }

    #[test]
    fn with_nothing_in_popular_now_the_featured_title_is_the_latest_released_and_ties_go_by_sort_title()
     {
        let titles = [
            released("Sicario", Some(date(2025, 5, 13))),
            released("Arrival", Some(date(2016, 11, 11))),
            released("Dune", None),
            released("Andor", Some(date(2025, 5, 13))),
        ];

        assert_eq!(featured_title(&[], &titles), Some(titles[3].id));
    }

    #[test]
    fn with_no_dated_titles_there_is_no_featured_title() {
        let titles = [released("Dune", None)];

        assert_eq!(featured_title(&[], &titles), None);
    }
}
