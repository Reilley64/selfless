use jiff::Timestamp;

pub fn first_added_at(
    stored: Option<Timestamp>,
    file_added_at: Option<Timestamp>,
) -> Option<Timestamp> {
    stored.or(file_added_at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(timestamp: &str) -> Timestamp {
        timestamp.parse().expect("the timestamp is valid")
    }

    #[test]
    fn a_replaced_file_keeps_the_date_the_first_file_was_added() {
        assert_eq!(
            first_added_at(
                Some(at("2026-10-29T10:00:00Z")),
                Some(at("2026-10-30T10:00:00Z"))
            ),
            Some(at("2026-10-29T10:00:00Z"))
        );
    }

    #[test]
    fn the_first_file_sets_the_date_added() {
        assert_eq!(
            first_added_at(None, Some(at("2026-10-29T10:00:00Z"))),
            Some(at("2026-10-29T10:00:00Z"))
        );
    }

    #[test]
    fn losing_the_file_keeps_the_date_added() {
        assert_eq!(
            first_added_at(Some(at("2026-10-29T10:00:00Z")), None),
            Some(at("2026-10-29T10:00:00Z"))
        );
    }
}
