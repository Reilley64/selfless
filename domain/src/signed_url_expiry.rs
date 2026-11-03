use jiff::Timestamp;
use jiff::ToSpan;
use jiff::tz::TimeZone;

pub fn signed_url_expiry(now: Timestamp) -> Timestamp {
    now.to_zoned(TimeZone::UTC)
        .date()
        .checked_add(2.days())
        .and_then(|x| x.to_zoned(TimeZone::UTC))
        .expect("two days after now is a valid UTC time")
        .timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(timestamp: &str) -> Timestamp {
        timestamp.parse().expect("the timestamp is valid")
    }

    #[test]
    fn a_signed_url_expires_at_the_end_of_the_next_utc_day() {
        assert_eq!(
            signed_url_expiry(at("2026-11-03T10:15:00Z")),
            at("2026-11-05T00:00:00Z")
        );
    }

    #[test]
    fn a_url_signed_just_before_midnight_still_lasts_the_whole_next_day() {
        assert_eq!(
            signed_url_expiry(at("2026-11-03T23:59:59Z")),
            at("2026-11-05T00:00:00Z")
        );
        assert_eq!(
            signed_url_expiry(at("2026-11-04T00:00:00Z")),
            at("2026-11-06T00:00:00Z")
        );
    }
}
