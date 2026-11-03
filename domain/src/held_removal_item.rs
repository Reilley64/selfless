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
    pub item_id: Uuid,
}
