use domain::User;

pub enum Output {
    SignedIn { token: String, user: User },
    WrongNameOrPassword,
}
