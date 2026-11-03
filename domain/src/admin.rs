use crate::LastAdmin;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct Admin {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub name: String,
    pub password_hash: String,
}

impl Admin {
    pub fn can_delete(admins: u64) -> Result<(), LastAdmin> {
        if admins < 2 {
            return Err(LastAdmin::new());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_admin_can_be_deleted_when_there_are_two_or_more_admins() {
        assert!(Admin::can_delete(2).is_ok());
        assert!(Admin::can_delete(3).is_ok());
    }

    #[test]
    fn the_last_admin_cannot_be_deleted() {
        assert!(Admin::can_delete(1).is_err());
    }
}
