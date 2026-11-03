use crate::Movie;
use crate::Show;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct Actor {
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
    pub name: String,
    pub role: Option<String>,
    pub order: Option<i64>,
}
