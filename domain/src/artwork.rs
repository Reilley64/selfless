use crate::ArtworkKind;
use crate::Episode;
use crate::Movie;
use crate::Season;
use crate::Show;
use jiff::Timestamp;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct Artwork {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub movie_id: Option<Uuid>,
    #[belongs_to(key = movie_id, references = id)]
    pub movie: toasty::Deferred<Option<Movie>>,
    #[index]
    pub show_id: Option<Uuid>,
    #[belongs_to(key = show_id, references = id)]
    pub show: toasty::Deferred<Option<Show>>,
    #[index]
    pub season_id: Option<Uuid>,
    #[belongs_to(key = season_id, references = id)]
    pub season: toasty::Deferred<Option<Season>>,
    #[index]
    pub episode_id: Option<Uuid>,
    #[belongs_to(key = episode_id, references = id)]
    pub episode: toasty::Deferred<Option<Episode>>,
    pub kind: ArtworkKind,
    pub path: String,
    pub modified_at: Timestamp,
}
