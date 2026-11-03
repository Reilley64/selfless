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

impl Actor {
    pub fn listing_order(&self) -> Option<i64> {
        self.order
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toasty::Deferred;

    fn actor(name: &str, order: Option<i64>) -> Actor {
        Actor {
            id: Uuid::nil(),
            movie_id: None,
            movie: Deferred::default(),
            show_id: None,
            show: Deferred::default(),
            name: name.to_owned(),
            role: None,
            order,
        }
    }

    #[test]
    fn actors_are_listed_by_their_order_with_unordered_actors_first() {
        let mut actors = [
            actor("Jeremy Renner", Some(1)),
            actor("Forest Whitaker", None),
            actor("Amy Adams", Some(0)),
        ];

        actors.sort_by_key(Actor::listing_order);

        assert_eq!(
            actors.map(|x| x.name),
            ["Forest Whitaker", "Amy Adams", "Jeremy Renner"]
        );
    }
}
