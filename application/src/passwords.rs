use argon2::Argon2;
use argon2::password_hash::Error as HashError;
use argon2::password_hash::PasswordHasher;
use argon2::password_hash::PasswordVerifier;
use argon2::password_hash::phc::PasswordHash;
use std::fmt;
use tokio::task;

const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$RcmBi3W1jo5bbjttQ9AnKQ$n5F4769mUJJfdOy7Iso26OAbwVCeTssL1oMeWYKepGk";

#[ohno::error]
#[display("failed to hash a password")]
pub(crate) struct PasswordError;

#[derive(Debug)]
pub struct EmptyPassword;

impl fmt::Display for EmptyPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the password is empty")
    }
}

impl std::error::Error for EmptyPassword {}

pub(crate) async fn hash(password: String) -> Result<String, PasswordError> {
    if password.is_empty() {
        return Err(PasswordError::caused_by(EmptyPassword));
    }
    task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|x: PasswordHash| x.to_string())
            .map_err(PasswordError::caused_by)
    })
    .await
    .map_err(PasswordError::caused_by)?
}

pub(crate) async fn verify(password: String, hash: Option<String>) -> Result<bool, PasswordError> {
    task::spawn_blocking(move || {
        let known = hash.is_some();
        let hash = PasswordHash::new(hash.as_deref().unwrap_or(DUMMY_HASH))
            .map_err(PasswordError::caused_by)?;
        match Argon2::default().verify_password(password.as_bytes(), &hash) {
            Ok(()) => Ok(known),
            Err(HashError::PasswordInvalid) => Ok(false),
            Err(error) => Err(PasswordError::caused_by(error)),
        }
    })
    .await
    .map_err(PasswordError::caused_by)?
}
