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

impl Season {
    pub fn in_library_filter() -> toasty::stmt::Expr<bool> {
        Self::fields().episodes().any(Episode::in_library_filter())
    }

    pub fn is_specials(number: i64) -> bool {
        number == 0
    }

    pub fn listing_order(&self) -> (bool, i64) {
        (Self::is_specials(self.number), self.number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toasty::Deferred;

    fn season(number: i64) -> Season {
        Season {
            id: Uuid::nil(),
            show_id: Uuid::nil(),
            show: Deferred::default(),
            number,
            episodes: Deferred::default(),
            artwork: Deferred::default(),
        }
    }

    #[test]
    fn specials_are_listed_after_the_numbered_seasons() {
        let mut seasons = [season(2), season(0), season(1)];

        seasons.sort_by_key(Season::listing_order);

        assert_eq!(seasons.map(|x| x.number), [1, 2, 0]);
    }
}
