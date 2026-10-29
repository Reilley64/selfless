#[ohno::error]
#[display("failed to resend an invite")]
pub struct Error;

#[ohno::error]
#[display("no pending invite has this ID")]
pub struct InviteNotFound;
