#[ohno::error]
#[display("failed to rename a user")]
pub struct Error;

#[ohno::error]
#[display("the name is empty")]
pub struct EmptyName;

#[ohno::error]
#[display("another user already has this name")]
pub struct NameTaken;
