use crate::browsing::Image;
use uuid::Uuid;

#[derive(Debug)]
pub enum Output {
    Episodes(Vec<Episode>),
    NotFound,
}

#[derive(Debug)]
pub struct Episode {
    pub id: Uuid,
    pub number: i64,
    pub title: String,
    pub runtime_minutes: Option<i64>,
    pub plot: Option<String>,
    pub thumb: Option<Image>,
    pub media_file_id: Option<Uuid>,
}
