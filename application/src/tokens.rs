use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::Digest;
use sha2::Sha256;

pub(crate) fn generate() -> String {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).expect("the operating system provides randomness");
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}
