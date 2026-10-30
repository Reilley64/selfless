use crate::EmptyPassword;
use std::fmt;

pub struct Password(String);

impl Password {
    pub fn new(value: String) -> Result<Self, EmptyPassword> {
        if value.is_empty() {
            return Err(EmptyPassword::new());
        }
        Ok(Self(value))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_password_returns_empty_password() {
        let _: EmptyPassword = Password::new(String::new()).expect_err("the password is empty");
    }
}
