use crate::EpisodeNfo;
use jiff::civil::Date;

#[derive(Clone, Debug, PartialEq)]
pub struct EpisodeMetadata {
    pub title: String,
    pub air_date: Option<Date>,
    pub overview: Option<String>,
    pub rating: Option<f64>,
}

impl EpisodeMetadata {
    pub fn from_sonarr(title: String, air_date: Option<Date>, overview: Option<String>) -> Self {
        Self {
            title,
            air_date,
            overview,
            rating: None,
        }
    }

    pub fn with_nfo(self, nfo: &EpisodeNfo) -> Self {
        Self {
            title: nfo.title.clone().unwrap_or(self.title),
            air_date: nfo.air_date.or(self.air_date),
            overview: nfo.plot.clone().or(self.overview),
            rating: nfo.rating,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn sonarr() -> EpisodeMetadata {
        EpisodeMetadata::from_sonarr(
            "Kassa".to_owned(),
            Some(date(2022, 9, 21)),
            Some("Sonarr's overview".to_owned()),
        )
    }

    #[test]
    fn without_an_nfo_an_episode_has_only_sonarrs_fields() {
        assert_eq!(
            sonarr(),
            EpisodeMetadata {
                title: "Kassa".to_owned(),
                air_date: Some(date(2022, 9, 21)),
                overview: Some("Sonarr's overview".to_owned()),
                rating: None,
            }
        );
    }

    #[test]
    fn the_nfos_fields_override_sonarrs() {
        let nfo = EpisodeNfo {
            title: Some("Kassa (Extended)".to_owned()),
            air_date: Some(date(2022, 9, 22)),
            plot: Some("The NFO's plot".to_owned()),
            rating: Some(7.5),
            ..EpisodeNfo::default()
        };

        assert_eq!(
            sonarr().with_nfo(&nfo),
            EpisodeMetadata {
                title: "Kassa (Extended)".to_owned(),
                air_date: Some(date(2022, 9, 22)),
                overview: Some("The NFO's plot".to_owned()),
                rating: Some(7.5),
            }
        );
    }

    #[test]
    fn sonarrs_title_air_date_and_overview_fill_what_the_nfo_leaves_out() {
        let metadata = sonarr().with_nfo(&EpisodeNfo::default());

        assert_eq!(metadata.title, "Kassa");
        assert_eq!(metadata.air_date, Some(date(2022, 9, 21)));
        assert_eq!(metadata.overview.as_deref(), Some("Sonarr's overview"));
    }
}
