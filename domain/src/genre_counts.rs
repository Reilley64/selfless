use std::collections::BTreeMap;

pub fn genre_counts<'a>(
    genres: impl IntoIterator<Item = &'a [String]>,
) -> BTreeMap<&'a str, usize> {
    let mut counts = BTreeMap::new();
    for name in genres.into_iter().flatten() {
        *counts.entry(name.as_str()).or_default() += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_with_several_genres_counts_once_in_each() {
        let genres = [
            vec!["Drama".to_owned(), "Mystery".to_owned()],
            vec!["Drama".to_owned()],
            vec![],
        ];

        let counts = genre_counts(genres.iter().map(Vec::as_slice));

        assert_eq!(
            counts.into_iter().collect::<Vec<_>>(),
            [("Drama", 2), ("Mystery", 1)]
        );
    }
}
