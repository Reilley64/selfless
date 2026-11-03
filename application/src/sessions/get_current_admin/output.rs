use uuid::Uuid;

#[derive(Debug)]
pub enum Output {
    SignedIn { admin_id: Uuid },
    NotSignedIn,
}
