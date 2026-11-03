use crate::HeldRemoval;
use crate::Source;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct HeldRemovalItem {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub source: Source,
    #[belongs_to(key = source, references = source)]
    pub held_removal: toasty::Deferred<HeldRemoval>,
    pub movie_id: Option<Uuid>,
    pub show_id: Option<Uuid>,
}
