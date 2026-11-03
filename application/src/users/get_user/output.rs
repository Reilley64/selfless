use uuid::Uuid;

#[derive(Debug)]
pub enum Output {
    Found { id: Uuid, name: String },
    NotFound,
}
