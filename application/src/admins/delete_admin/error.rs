#[ohno::error]
#[display("failed to delete an admin")]
pub struct Error;

#[ohno::error]
#[display("the last admin cannot be deleted")]
pub struct LastAdmin;
