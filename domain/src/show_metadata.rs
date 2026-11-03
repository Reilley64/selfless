use crate::ShowNfo;
use jiff::civil::Date;

#[derive(Clone, Debug, PartialEq)]
pub struct ShowMetadata {
    pub title: String,
    pub sort_title: Option<String>,
    pub overview: Option<String>,
    pub certification: Option<String>,
    pub premiered: Option<Date>,
    pub end_date: Option<Date>,
    pub status: Option<String>,
    pub network: Option<String>,
    pub genres: Vec<String>,
    pub rating: Option<f64>,
}

impl ShowMetadata {
    pub fn from_sonarr(title: String, overview: Option<String>, genres: Vec<String>) -> Self {
        Self {
            title,
            sort_title: None,
            overview,
            certification: None,
            premiered: None,
            end_date: None,
            status: None,
            network: None,
            genres,
            rating: None,
        }
    }

    pub fn with_nfo(self, nfo: &ShowNfo) -> Self {
        Self {
            title: nfo.title.clone().unwrap_or(self.title),
            sort_title: nfo.sort_title.clone(),
            overview: nfo.plot.clone().or(self.overview),
            certification: nfo.certification.clone(),
            premiered: nfo.premiered,
            end_date: nfo.end_date,
            status: nfo.status.clone(),
            network: nfo.network.clone(),
            genres: if nfo.genres.is_empty() {
                self.genres
            } else {
                nfo.genres.clone()
            },
            rating: nfo.rating,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn sonarr() -> ShowMetadata {
        ShowMetadata::from_sonarr(
            "Andor".to_owned(),
            Some("Sonarr's overview".to_owned()),
            vec!["Action".to_owned(), "Drama".to_owned()],
        )
    }

    #[test]
    fn without_an_nfo_a_show_has_only_sonarrs_fields() {
        assert_eq!(
            sonarr(),
            ShowMetadata {
                title: "Andor".to_owned(),
                sort_title: None,
                overview: Some("Sonarr's overview".to_owned()),
                certification: None,
                premiered: None,
                end_date: None,
                status: None,
                network: None,
                genres: vec!["Action".to_owned(), "Drama".to_owned()],
                rating: None,
            }
        );
    }

    #[test]
    fn the_nfos_fields_override_sonarrs() {
        let nfo = ShowNfo {
            title: Some("Andor (Extended)".to_owned()),
            sort_title: Some("andor".to_owned()),
            plot: Some("The NFO's plot".to_owned()),
            certification: Some("TV-14".to_owned()),
            premiered: Some(date(2022, 9, 21)),
            end_date: Some(date(2025, 5, 13)),
            status: Some("Ended".to_owned()),
            network: Some("Disney+".to_owned()),
            genres: vec!["Science Fiction".to_owned()],
            rating: Some(8.4),
            actors: Vec::new(),
        };

        assert_eq!(
            sonarr().with_nfo(&nfo),
            ShowMetadata {
                title: "Andor (Extended)".to_owned(),
                sort_title: Some("andor".to_owned()),
                overview: Some("The NFO's plot".to_owned()),
                certification: Some("TV-14".to_owned()),
                premiered: Some(date(2022, 9, 21)),
                end_date: Some(date(2025, 5, 13)),
                status: Some("Ended".to_owned()),
                network: Some("Disney+".to_owned()),
                genres: vec!["Science Fiction".to_owned()],
                rating: Some(8.4),
            }
        );
    }

    #[test]
    fn sonarrs_title_overview_and_genres_fill_what_the_nfo_leaves_out() {
        let metadata = sonarr().with_nfo(&ShowNfo::default());

        assert_eq!(metadata.title, "Andor");
        assert_eq!(metadata.overview.as_deref(), Some("Sonarr's overview"));
        assert_eq!(metadata.genres, ["Action", "Drama"]);
    }
}
