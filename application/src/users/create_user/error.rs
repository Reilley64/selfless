#[ohno::error]
#[display("failed to create a user")]
pub struct Error;

#[ohno::error]
#[display("a user with this name already exists")]
pub struct NameTaken;
