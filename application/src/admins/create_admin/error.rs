#[ohno::error]
#[display("failed to create an admin")]
pub struct Error;

#[ohno::error]
#[display("an admin with this name already exists")]
pub struct NameTaken;
