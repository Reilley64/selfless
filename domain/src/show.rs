use crate::Actor;
use crate::Artwork;
use crate::Episode;
use crate::Season;
use crate::ShowMetadata;
use crate::SyncProblem;
use jiff::Timestamp;
use jiff::civil::Date;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct Show {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub tvdb_id: i64,
    #[unique]
    pub sonarr_id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub genres: Vec<String>,
    pub overview: Option<String>,
    pub sort_title: Option<String>,
    pub certification: Option<String>,
    pub premiered: Option<Date>,
    pub end_date: Option<Date>,
    pub status: Option<String>,
    pub network: Option<String>,
    pub rating: Option<f64>,
    pub nfo_modified_at: Option<Timestamp>,
    #[has_many]
    pub seasons: toasty::Deferred<Vec<Season>>,
    #[has_many]
    pub actors: toasty::Deferred<Vec<Actor>>,
    #[has_many]
    pub artwork: toasty::Deferred<Vec<Artwork>>,
    #[has_many]
    pub sync_problems: toasty::Deferred<Vec<SyncProblem>>,
}

impl Show {
    pub fn in_library_filter() -> toasty::stmt::Expr<bool> {
        Self::fields().seasons().any(
            Season::fields()
                .episodes()
                .any(Episode::in_library_filter()),
        )
    }

    pub fn metadata(&self) -> ShowMetadata {
        ShowMetadata {
            title: self.title.clone(),
            sort_title: self.sort_title.clone(),
            overview: self.overview.clone(),
            certification: self.certification.clone(),
            premiered: self.premiered,
            end_date: self.end_date,
            status: self.status.clone(),
            network: self.network.clone(),
            genres: self.genres.clone(),
            rating: self.rating,
        }
    }
}
