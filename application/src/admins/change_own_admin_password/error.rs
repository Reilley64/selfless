#[ohno::error]
#[display("failed to change an admin's own password")]
pub struct Error;

#[ohno::error]
#[display("the current password is wrong")]
pub struct WrongCurrentPassword;
