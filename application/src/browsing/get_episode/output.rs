use crate::browsing::Image;
use domain::SubtitleLanguage;
use jiff::civil::Date;
use uuid::Uuid;

#[derive(Debug)]
pub enum Output {
    Episode(Box<Episode>),
    NotFound,
}

#[derive(Debug)]
pub struct Episode {
    pub id: Uuid,
    pub title: String,
    pub season_number: i64,
    pub episode_number: i64,
    pub air_date: Option<Date>,
    pub runtime_minutes: Option<i64>,
    pub rating: Option<f64>,
    pub plot: Option<String>,
    pub thumb: Option<Image>,
    pub show: Show,
    pub audio_languages: Vec<String>,
    pub subtitle_languages: Vec<SubtitleLanguage>,
    pub resume_position_seconds: Option<i64>,
    pub progress: Option<f64>,
    pub minutes_left: Option<i64>,
    pub watched: bool,
}

#[derive(Debug)]
pub struct Show {
    pub id: Uuid,
    pub title: String,
    pub logo: Option<Image>,
}
