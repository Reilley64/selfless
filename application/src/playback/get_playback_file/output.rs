#[derive(Debug)]
pub enum Output {
    File { path: String },
    NotFound,
    InvalidSignature,
}
