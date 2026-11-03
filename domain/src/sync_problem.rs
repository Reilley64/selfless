use crate::MediaFile;
use crate::Movie;
use crate::Owner;
use crate::Show;
use crate::SyncProblemKind;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct SyncProblem {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub movie_id: Option<Uuid>,
    #[belongs_to(key = movie_id, references = id)]
    pub movie: toasty::Deferred<Option<Movie>>,
    #[index]
    pub media_file_id: Option<Uuid>,
    #[belongs_to(key = media_file_id, references = id)]
    pub media_file: toasty::Deferred<Option<MediaFile>>,
    #[index]
    pub show_id: Option<Uuid>,
    #[belongs_to(key = show_id, references = id)]
    pub show: toasty::Deferred<Option<Show>>,
    pub kind: SyncProblemKind,
    pub path: String,
}

impl SyncProblem {
    pub fn owner_filter(owner: Owner) -> toasty::stmt::Expr<bool> {
        match owner {
            Owner::Movie(id) => Self::fields().movie_id().eq(id),
            Owner::Show(id) => Self::fields().show_id().eq(id),
        }
    }
}
