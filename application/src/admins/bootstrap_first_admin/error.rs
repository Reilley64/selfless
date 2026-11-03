#[ohno::error]
#[display("failed to create the first admin")]
pub struct Error;

#[ohno::error]
#[display("no admin exists, and the first admin's name or password is missing")]
pub struct MissingFirstAdmin;
