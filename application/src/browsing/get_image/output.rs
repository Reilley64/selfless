use crate::ports::ImageFile;
use jiff::SignedDuration;

#[derive(Debug)]
pub enum Output {
    Image {
        file: ImageFile,
        max_age: SignedDuration,
    },
    NotFound,
    InvalidSignature,
}
