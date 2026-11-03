use crate::HeldRemoval;
use crate::Owner;
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

impl HeldRemovalItem {
    pub fn owner(&self) -> Option<Owner> {
        self.movie_id
            .map(Owner::Movie)
            .or(self.show_id.map(Owner::Show))
    }
}
