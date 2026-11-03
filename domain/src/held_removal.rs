use crate::HeldRemovalItem;
use crate::Source;

#[derive(Debug, toasty::Model)]
pub struct HeldRemoval {
    #[key]
    pub source: Source,
    #[has_many]
    pub items: toasty::Deferred<Vec<HeldRemovalItem>>,
}

impl HeldRemoval {
    pub fn holds(absent: usize, stored: usize) -> bool {
        absent * 2 > stored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_removal_of_more_than_half_the_stored_items_is_held() {
        assert!(HeldRemoval::holds(3, 4));
    }

    #[test]
    fn a_removal_of_exactly_half_the_stored_items_is_not_held() {
        assert!(!HeldRemoval::holds(2, 4));
    }

    #[test]
    fn a_removal_of_every_stored_item_is_held() {
        assert!(HeldRemoval::holds(4, 4));
    }
}
