use jiff::Timestamp;

pub enum Nfo<T> {
    Missing,
    Unchanged,
    Read(T, Timestamp),
    Unreadable(String),
}

impl<T> Nfo<T> {
    pub fn metadata<M>(
        &self,
        stored: Option<(M, Option<Timestamp>)>,
        source: M,
        with_nfo: impl FnOnce(M, &T) -> M,
    ) -> (M, Option<Timestamp>) {
        match (self, stored) {
            (Nfo::Unchanged, Some(stored)) => stored,
            (Nfo::Read(nfo, modified_at), _) => (with_nfo(source, nfo), Some(*modified_at)),
            _ => (source, None),
        }
    }

    pub fn unreadable_path(&self) -> Option<&str> {
        match self {
            Nfo::Unreadable(path) => Some(path),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MovieMetadata;
    use crate::MovieNfo;

    const STORED_AT: &str = "2026-10-29T10:00:00Z";
    const READ_AT: &str = "2026-10-30T10:00:00Z";

    fn at(timestamp: &str) -> Timestamp {
        timestamp.parse().expect("the timestamp is valid")
    }

    fn metadata(title: &str) -> MovieMetadata {
        MovieMetadata::from_source(title.to_owned(), 2016, None, Vec::new())
    }

    fn stored() -> Option<(MovieMetadata, Option<Timestamp>)> {
        Some((metadata("Arrival (stored)"), Some(at(STORED_AT))))
    }

    fn title_and_modified_at(nfo: Nfo<MovieNfo>) -> (String, Option<Timestamp>) {
        let (metadata, modified_at) =
            nfo.metadata(stored(), metadata("Arrival"), MovieMetadata::with_nfo);
        (metadata.title, modified_at)
    }

    #[test]
    fn an_unchanged_nfo_keeps_the_stored_metadata_and_modification_time() {
        assert_eq!(
            title_and_modified_at(Nfo::Unchanged),
            ("Arrival (stored)".to_owned(), Some(at(STORED_AT)))
        );
    }

    #[test]
    fn a_read_nfo_fills_the_sources_metadata_and_records_its_modification_time() {
        let nfo = MovieNfo {
            title: Some("Arrival (NFO)".to_owned()),
            ..MovieNfo::default()
        };

        assert_eq!(
            title_and_modified_at(Nfo::Read(nfo, at(READ_AT))),
            ("Arrival (NFO)".to_owned(), Some(at(READ_AT)))
        );
    }

    #[test]
    fn a_missing_or_unreadable_nfo_falls_back_to_the_sources_metadata() {
        for nfo in [
            Nfo::Missing,
            Nfo::Unreadable("/media/Movies/Arrival (2016)/movie.nfo".to_owned()),
        ] {
            assert_eq!(title_and_modified_at(nfo), ("Arrival".to_owned(), None));
        }
    }

    #[test]
    fn an_unchanged_nfo_without_a_stored_item_falls_back_to_the_sources_metadata() {
        let (metadata, modified_at) =
            Nfo::<MovieNfo>::Unchanged.metadata(None, metadata("Arrival"), MovieMetadata::with_nfo);

        assert_eq!((metadata.title.as_str(), modified_at), ("Arrival", None));
    }
}
