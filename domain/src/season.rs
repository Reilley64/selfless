use crate::Artwork;
use crate::Episode;
use crate::Show;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct Season {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub show_id: Uuid,
    #[belongs_to(key = show_id, references = id)]
    pub show: toasty::Deferred<Show>,
    pub number: i64,
    #[has_many]
    pub episodes: toasty::Deferred<Vec<Episode>>,
    #[has_many]
    pub artwork: toasty::Deferred<Vec<Artwork>>,
}
