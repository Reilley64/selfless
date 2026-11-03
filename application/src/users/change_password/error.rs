#[ohno::error]
#[display("failed to change a user's password")]
pub struct Error;

#[ohno::error]
#[display("the current password is wrong")]
pub struct WrongCurrentPassword;
