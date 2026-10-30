#[ohno::error]
#[display("failed to accept an invite")]
pub struct Error;

#[ohno::error]
#[display("no pending invite has this token, or it has expired")]
pub struct InviteNotFound;
