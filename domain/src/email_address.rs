use crate::InvalidEmailAddress;
use ::email_address::Options;
use std::fmt;

#[derive(Debug, Clone, toasty::Embed)]
pub struct EmailAddress(String);

impl EmailAddress {
    pub fn new(value: String) -> Result<Self, InvalidEmailAddress> {
        let value = value.trim();
        ::email_address::EmailAddress::parse_with_options(
            value,
            Options::default().without_display_text(),
        )
        .map_err(|_| InvalidEmailAddress::new())?;
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_trimmed_and_keeps_its_capitals() {
        let address = EmailAddress::new(" Noor@Household.example\n".to_owned())
            .expect("the address is valid");

        assert_eq!(address.as_str(), "Noor@Household.example");
    }

    #[test]
    fn an_address_that_is_not_valid_returns_invalid_email_address() {
        let _: InvalidEmailAddress =
            EmailAddress::new("not an address".to_owned()).expect_err("the address is not valid");
    }

    #[test]
    fn an_address_with_a_display_name_is_refused() {
        let _: InvalidEmailAddress = EmailAddress::new("Noor <noor@household.example>".to_owned())
            .expect_err("the address has a display name");
    }
}
