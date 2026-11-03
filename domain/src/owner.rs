use uuid::Uuid;

#[derive(Clone, Copy, Debug)]
pub enum Owner {
    Movie(Uuid),
    Show(Uuid),
}

impl Owner {
    pub fn movie_id(self) -> Option<Uuid> {
        match self {
            Owner::Movie(id) => Some(id),
            Owner::Show(_) => None,
        }
    }

    pub fn show_id(self) -> Option<Uuid> {
        match self {
            Owner::Movie(_) => None,
            Owner::Show(id) => Some(id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: Uuid = Uuid::from_u128(1);

    #[test]
    fn a_movie_owner_sets_only_the_movie_key() {
        let owner = Owner::Movie(ID);

        assert_eq!((owner.movie_id(), owner.show_id()), (Some(ID), None));
    }

    #[test]
    fn a_show_owner_sets_only_the_show_key() {
        let owner = Owner::Show(ID);

        assert_eq!((owner.movie_id(), owner.show_id()), (None, Some(ID)));
    }
}
