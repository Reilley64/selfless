use crate::NfoActor;
use jiff::civil::Date;

#[derive(Clone, Debug, Default)]
pub struct ShowNfo {
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub plot: Option<String>,
    pub certification: Option<String>,
    pub premiered: Option<Date>,
    pub end_date: Option<Date>,
    pub status: Option<String>,
    pub network: Option<String>,
    pub genres: Vec<String>,
    pub rating: Option<f64>,
    pub actors: Vec<NfoActor>,
}
