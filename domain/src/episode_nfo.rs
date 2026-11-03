use jiff::civil::Date;
use std::collections::HashSet;

#[derive(Clone, Debug, Default)]
pub struct EpisodeNfo {
    pub sonarr_id: Option<i64>,
    pub title: Option<String>,
    pub season_number: Option<i64>,
    pub episode_number: Option<i64>,
    pub air_date: Option<Date>,
    pub overview: Option<String>,
    pub rating: Option<f64>,
}

impl EpisodeNfo {
    pub fn matching<'a>(
        entries: &'a [EpisodeNfo],
        sonarr_id: i64,
        season_number: i64,
        episode_number: i64,
        show_sonarr_ids: &HashSet<i64>,
    ) -> Option<&'a EpisodeNfo> {
        entries
            .iter()
            .find(|x| x.sonarr_id == Some(sonarr_id))
            .or_else(|| {
                entries.iter().find(|x| {
                    !x.sonarr_id.is_some_and(|x| show_sonarr_ids.contains(&x))
                        && x.season_number == Some(season_number)
                        && x.episode_number == Some(episode_number)
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(sonarr_id: Option<i64>, season_number: i64, episode_number: i64) -> EpisodeNfo {
        EpisodeNfo {
            sonarr_id,
            title: Some(format!("S{season_number:02}E{episode_number:02}")),
            season_number: Some(season_number),
            episode_number: Some(episode_number),
            ..EpisodeNfo::default()
        }
    }

    fn matching(
        entries: &[EpisodeNfo],
        sonarr_id: i64,
        season_number: i64,
        episode_number: i64,
        show_sonarr_ids: &[i64],
    ) -> Option<String> {
        let show_sonarr_ids = show_sonarr_ids.iter().copied().collect();
        EpisodeNfo::matching(
            entries,
            sonarr_id,
            season_number,
            episode_number,
            &show_sonarr_ids,
        )
        .and_then(|x| x.title.clone())
    }

    #[test]
    fn an_entry_with_the_episodes_sonarr_id_matches_whatever_its_numbers() {
        let entries = [entry(Some(1012), 1, 2), entry(Some(1021), 3, 9)];

        assert_eq!(
            matching(&entries, 1021, 2, 1, &[1012, 1021]).as_deref(),
            Some("S03E09")
        );
    }

    #[test]
    fn a_sonarr_id_match_wins_over_a_number_match() {
        let entries = [entry(None, 1, 2), entry(Some(1012), 1, 3)];

        assert_eq!(
            matching(&entries, 1012, 1, 2, &[1012]).as_deref(),
            Some("S01E03")
        );
    }

    #[test]
    fn an_entry_without_a_sonarr_id_matches_by_season_and_episode_number() {
        let entries = [entry(None, 0, 2), entry(None, 0, 1)];

        assert_eq!(
            matching(&entries, 1001, 0, 1, &[1001]).as_deref(),
            Some("S00E01")
        );
    }

    #[test]
    fn an_entry_of_a_re_added_show_matches_by_season_and_episode_number() {
        let entries = [entry(Some(1012), 1, 2), entry(Some(1013), 1, 3)];

        assert_eq!(
            matching(&entries, 11_013, 1, 3, &[11_012, 11_013]).as_deref(),
            Some("S01E03")
        );
    }

    #[test]
    fn an_entry_whose_sonarr_id_belongs_to_another_episode_is_not_matched_by_number() {
        let entries = [entry(Some(1013), 1, 3)];

        assert_eq!(matching(&entries, 1014, 1, 3, &[1013, 1014]), None);
    }
}
