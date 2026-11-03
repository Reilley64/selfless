use std::time::Duration;

const MAX_KEYFRAME_GAP: Duration = Duration::from_secs(20);

pub fn usable_keyframe_index(keyframes: &[Duration]) -> bool {
    !keyframes.is_empty()
        && keyframes
            .iter()
            .try_fold(Duration::ZERO, |previous, &x| {
                (x.saturating_sub(previous) <= MAX_KEYFRAME_GAP).then_some(x)
            })
            .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(values: &[f64]) -> Vec<Duration> {
        values.iter().map(|x| Duration::from_secs_f64(*x)).collect()
    }

    #[test]
    fn an_index_with_a_keyframe_at_least_every_20_seconds_is_usable() {
        assert!(usable_keyframe_index(&seconds(&[0.0, 20.0, 35.5, 55.5])));
    }

    #[test]
    fn a_gap_over_20_seconds_between_keyframes_makes_an_index_unusable() {
        assert!(!usable_keyframe_index(&seconds(&[0.0, 10.0, 30.1, 40.0])));
    }

    #[test]
    fn a_first_keyframe_after_20_seconds_makes_an_index_unusable() {
        assert!(!usable_keyframe_index(&seconds(&[20.1, 30.0])));
    }

    #[test]
    fn an_index_without_keyframes_is_unusable() {
        assert!(!usable_keyframe_index(&[]));
    }
}
