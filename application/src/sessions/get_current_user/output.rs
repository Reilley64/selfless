use domain::User;

#[derive(Debug)]
pub enum Output {
    SignedIn { user: User },
    NotSignedIn,
}
