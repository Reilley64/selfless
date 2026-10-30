#[ohno::error]
#[display("failed to create an invite")]
pub struct Error;

#[ohno::error]
#[display("a user or a pending invite already has this email address")]
pub struct AddressTaken;
