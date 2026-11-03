use crate::MovieNfo;
use jiff::civil::Date;

#[derive(Clone, Debug, PartialEq)]
pub struct MovieMetadata {
    pub title: String,
    pub sort_title: Option<String>,
    pub original_title: Option<String>,
    pub year: i64,
    pub overview: Option<String>,
    pub runtime_minutes: Option<i64>,
    pub certification: Option<String>,
    pub premiered: Option<Date>,
    pub studio: Option<String>,
    pub genres: Vec<String>,
    pub rating: Option<f64>,
    pub directors: Vec<String>,
}

impl MovieMetadata {
    pub fn from_source(
        title: String,
        year: i64,
        overview: Option<String>,
        genres: Vec<String>,
    ) -> Self {
        Self {
            title,
            sort_title: None,
            original_title: None,
            year,
            overview,
            runtime_minutes: None,
            certification: None,
            premiered: None,
            studio: None,
            genres,
            rating: None,
            directors: Vec::new(),
        }
    }

    pub fn with_nfo(self, nfo: &MovieNfo) -> Self {
        Self {
            title: nfo.title.clone().unwrap_or(self.title),
            sort_title: nfo.sort_title.clone(),
            original_title: nfo.original_title.clone(),
            year: nfo.year.unwrap_or(self.year),
            overview: nfo.overview.clone().or(self.overview),
            runtime_minutes: nfo.runtime_minutes,
            certification: nfo.certification.clone(),
            premiered: nfo.premiered,
            studio: nfo.studio.clone(),
            genres: if nfo.genres.is_empty() {
                self.genres
            } else {
                nfo.genres.clone()
            },
            rating: nfo.rating,
            directors: nfo.directors.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn radarr() -> MovieMetadata {
        MovieMetadata::from_source(
            "Arrival".to_owned(),
            2016,
            Some("Radarr's overview".to_owned()),
            vec!["Drama".to_owned(), "Science Fiction".to_owned()],
        )
    }

    #[test]
    fn without_an_nfo_a_movie_has_only_radarrs_fields() {
        assert_eq!(
            radarr(),
            MovieMetadata {
                title: "Arrival".to_owned(),
                sort_title: None,
                original_title: None,
                year: 2016,
                overview: Some("Radarr's overview".to_owned()),
                runtime_minutes: None,
                certification: None,
                premiered: None,
                studio: None,
                genres: vec!["Drama".to_owned(), "Science Fiction".to_owned()],
                rating: None,
                directors: Vec::new(),
            }
        );
    }

    #[test]
    fn the_nfos_fields_override_radarrs() {
        let nfo = MovieNfo {
            title: Some("Arrival (Director's Cut)".to_owned()),
            sort_title: Some("arrival".to_owned()),
            original_title: Some("Story of Your Life".to_owned()),
            year: Some(2017),
            overview: Some("The NFO's overview".to_owned()),
            runtime_minutes: Some(116),
            certification: Some("PG-13".to_owned()),
            premiered: Some(date(2016, 11, 10)),
            studio: Some("FilmNation Entertainment".to_owned()),
            genres: vec!["Mystery".to_owned()],
            rating: Some(7.6),
            directors: vec!["Denis Villeneuve".to_owned()],
            actors: Vec::new(),
        };

        assert_eq!(
            radarr().with_nfo(&nfo),
            MovieMetadata {
                title: "Arrival (Director's Cut)".to_owned(),
                sort_title: Some("arrival".to_owned()),
                original_title: Some("Story of Your Life".to_owned()),
                year: 2017,
                overview: Some("The NFO's overview".to_owned()),
                runtime_minutes: Some(116),
                certification: Some("PG-13".to_owned()),
                premiered: Some(date(2016, 11, 10)),
                studio: Some("FilmNation Entertainment".to_owned()),
                genres: vec!["Mystery".to_owned()],
                rating: Some(7.6),
                directors: vec!["Denis Villeneuve".to_owned()],
            }
        );
    }

    #[test]
    fn radarrs_title_year_overview_and_genres_fill_what_the_nfo_leaves_out() {
        let metadata = radarr().with_nfo(&MovieNfo::default());

        assert_eq!(metadata.title, "Arrival");
        assert_eq!(metadata.year, 2016);
        assert_eq!(metadata.overview.as_deref(), Some("Radarr's overview"));
        assert_eq!(metadata.genres, ["Drama", "Science Fiction"]);
    }
}
