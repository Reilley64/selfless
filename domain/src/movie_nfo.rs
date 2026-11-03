use crate::NfoActor;
use jiff::civil::Date;

#[derive(Clone, Debug, Default)]
pub struct MovieNfo {
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub overview: Option<String>,
    pub runtime_minutes: Option<i64>,
    pub certification: Option<String>,
    pub premiered: Option<Date>,
    pub studio: Option<String>,
    pub genres: Vec<String>,
    pub rating: Option<f64>,
    pub directors: Vec<String>,
    pub actors: Vec<NfoActor>,
}
