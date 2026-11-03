use jiff::Timestamp;

pub fn needs_reread(stored_modified_at: Option<Timestamp>, modified_at: Timestamp) -> bool {
    stored_modified_at != Some(modified_at)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODIFIED_AT: &str = "2026-10-29T10:00:00Z";

    fn at(timestamp: &str) -> Timestamp {
        timestamp.parse().expect("the timestamp is valid")
    }

    #[test]
    fn a_file_with_the_stored_modification_time_needs_no_reread() {
        assert!(!needs_reread(Some(at(MODIFIED_AT)), at(MODIFIED_AT)));
    }

    #[test]
    fn a_file_whose_modification_time_changed_needs_a_reread() {
        assert!(needs_reread(
            Some(at(MODIFIED_AT)),
            at("2026-10-30T10:00:00Z")
        ));
    }

    #[test]
    fn a_file_never_read_needs_a_reread() {
        assert!(needs_reread(None, at(MODIFIED_AT)));
    }
}
