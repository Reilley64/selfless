use crate::Colour;
use crate::EmailAddress;
use jiff::Timestamp;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct User {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub name: String,
    #[unique]
    pub email: EmailAddress,
    pub password_hash: String,
    pub idempotency_key: Option<Uuid>,
    pub created_at: Timestamp,
}

impl User {
    pub fn colour(&self) -> Colour {
        let tail = self.id.as_bytes()[8..]
            .try_into()
            .expect("a UUID has 16 bytes");
        let index = u64::from_be_bytes(tail) % Colour::ALL.len() as u64;
        Colour::ALL[index as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(id: &str) -> User {
        User {
            id: id.parse().expect("the ID is valid"),
            name: "Noor".to_owned(),
            email: EmailAddress::new("noor@household.example".to_owned())
                .expect("the address is valid"),
            password_hash: String::new(),
            idempotency_key: None,
            created_at: Timestamp::UNIX_EPOCH,
        }
    }

    #[test]
    fn consecutive_id_tails_give_the_colours_in_order() {
        let colours = [
            "019a3b2c-0000-7000-8000-000000000054",
            "019a3b2c-0000-7000-8000-000000000055",
            "019a3b2c-0000-7000-8000-000000000056",
            "019a3b2c-0000-7000-8000-000000000057",
        ]
        .map(|x| user(x).colour());

        assert_eq!(colours, Colour::ALL);
    }

    #[test]
    fn the_colour_ignores_the_timestamp_in_the_id() {
        let earlier = user("01900000-0000-7000-8000-0000000abc55");
        let later = user("019a3b2c-4d5e-7000-8000-0000000abc55");

        assert_eq!(earlier.colour(), later.colour());
    }
}
