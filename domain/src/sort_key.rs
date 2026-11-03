pub fn sort_key(sort_title: Option<&str>, title: &str) -> (String, String) {
    (sort_title.unwrap_or(title).to_lowercase(), title.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted<'a, const N: usize>(mut titles: [(&'a str, Option<&str>); N]) -> [&'a str; N] {
        titles.sort_by_cached_key(|(title, sort_title)| sort_key(*sort_title, title));
        titles.map(|(title, _)| title)
    }

    #[test]
    fn a_title_sorts_by_its_sort_title_when_it_has_one() {
        assert_eq!(
            sorted([
                ("Sicario", None),
                ("Dune", None),
                ("The Arrival", Some("Arrival")),
                ("Blade Runner 2049", None),
            ]),
            ["The Arrival", "Blade Runner 2049", "Dune", "Sicario"]
        );
        assert_eq!(
            sorted([("Andor", Some("Star Wars: Andor")), ("Severance", None)]),
            ["Severance", "Andor"]
        );
    }
}
