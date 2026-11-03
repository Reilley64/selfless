use jiff::civil::Date;

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
