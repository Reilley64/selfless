use crate::Artwork;
use crate::EpisodeMetadata;
use crate::MediaFile;
use crate::Season;
use jiff::Timestamp;
use jiff::civil::Date;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct Episode {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub season_id: Uuid,
    #[belongs_to(key = season_id, references = id)]
    pub season: toasty::Deferred<Season>,
    #[unique]
    pub tvdb_id: i64,
    pub number: i64,
    pub title: String,
    pub air_date: Option<Date>,
    pub overview: Option<String>,
    pub added_at: Option<Timestamp>,
    #[index]
    pub media_file_id: Option<Uuid>,
    #[belongs_to(key = media_file_id, references = id)]
    pub media_file: toasty::Deferred<Option<MediaFile>>,
    pub visible: bool,
    pub rating: Option<f64>,
    pub nfo_modified_at: Option<Timestamp>,
    #[has_many]
    pub artwork: toasty::Deferred<Vec<Artwork>>,
}

impl Episode {
    pub fn in_library(sonarr_has_file: bool, file_on_disk: bool) -> bool {
        sonarr_has_file && file_on_disk
    }

    pub fn in_library_filter() -> toasty::stmt::Expr<bool> {
        Self::fields().visible().eq(true)
    }

    pub fn metadata(&self) -> EpisodeMetadata {
        EpisodeMetadata {
            title: self.title.clone(),
            air_date: self.air_date,
            overview: self.overview.clone(),
            rating: self.rating,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_episode_is_in_the_library_when_sonarr_has_its_file_and_the_file_is_on_disk() {
        assert!(Episode::in_library(true, true));
    }

    #[test]
    fn an_episode_sonarr_has_no_file_for_is_not_in_the_library() {
        assert!(!Episode::in_library(false, false));
    }

    #[test]
    fn an_episode_whose_file_is_missing_on_disk_is_not_in_the_library() {
        assert!(!Episode::in_library(true, false));
    }
}
