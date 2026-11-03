use crate::Source;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct HeldRemoval {
    #[key]
    pub source: Source,
    pub item_ids: Vec<Uuid>,
}
