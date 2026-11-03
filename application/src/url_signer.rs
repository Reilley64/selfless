use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use domain::signed_url_expiry;
use hmac::Hmac;
use hmac::KeyInit;
use hmac::Mac;
use jiff::Timestamp;
use sha2::Sha256;
use std::sync::Arc;

#[derive(Clone)]
pub struct UrlSigner {
    key: Arc<[u8]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedUrl {
    pub expires: Timestamp,
    pub signature: String,
}

impl UrlSigner {
    pub fn new(key: &[u8]) -> Self {
        Self { key: key.into() }
    }

    pub fn sign(&self, resource: &str, now: Timestamp) -> SignedUrl {
        let expires = signed_url_expiry(now);
        SignedUrl {
            expires,
            signature: self.signature(&expiring(resource, expires)),
        }
    }

    pub fn signature(&self, resource: &str) -> String {
        URL_SAFE_NO_PAD.encode(self.mac(resource).finalize().into_bytes())
    }

    pub fn verify(
        &self,
        resource: &str,
        expires: Timestamp,
        signature: &str,
        now: Timestamp,
    ) -> bool {
        now < expires && self.verify_signature(&expiring(resource, expires), signature)
    }

    pub fn verify_signature(&self, resource: &str, signature: &str) -> bool {
        URL_SAFE_NO_PAD
            .decode(signature)
            .is_ok_and(|x| self.mac(resource).verify_slice(&x).is_ok())
    }

    fn mac(&self, message: &str) -> Hmac<Sha256> {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&self.key).expect("HMAC takes a key of any length");
        mac.update(message.as_bytes());
        mac
    }
}

fn expiring(resource: &str, expires: Timestamp) -> String {
    format!("{resource}\n{}", expires.as_second())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::SignedDuration;

    const RESOURCE: &str = "images/0190f3c4-7d8e-7f90-a1b2-c3d4e5f6a7b8/480/1793664000000000000";

    fn at(timestamp: &str) -> Timestamp {
        timestamp.parse().expect("the timestamp is valid")
    }

    #[test]
    fn a_signed_url_verifies_until_the_end_of_the_next_utc_day() {
        let signer = UrlSigner::new(b"a household secret");
        let now = at("2026-11-03T10:15:00Z");

        let url = signer.sign(RESOURCE, now);

        assert_eq!(url.expires, at("2026-11-05T00:00:00Z"));
        assert!(signer.verify(RESOURCE, url.expires, &url.signature, now));
        assert!(signer.verify(
            RESOURCE,
            url.expires,
            &url.signature,
            url.expires - SignedDuration::from_secs(1)
        ));
        assert!(!signer.verify(RESOURCE, url.expires, &url.signature, url.expires));
    }

    #[test]
    fn a_signature_signs_the_resource_and_the_expiry_with_the_key() {
        let signer = UrlSigner::new(b"a household secret");
        let now = at("2026-11-03T10:15:00Z");
        let url = signer.sign(RESOURCE, now);
        let mut tampered = url.signature.clone();
        tampered.replace_range(..1, if tampered.starts_with('A') { "B" } else { "A" });

        assert!(!signer.verify(RESOURCE, url.expires, &tampered, now));
        assert!(!signer.verify(&format!("{RESOURCE}0"), url.expires, &url.signature, now));
        assert!(!signer.verify(
            RESOURCE,
            url.expires + SignedDuration::from_hours(24),
            &url.signature,
            now
        ));
        assert!(!UrlSigner::new(b"another secret").verify(
            RESOURCE,
            url.expires,
            &url.signature,
            now
        ));
        assert!(!signer.verify(RESOURCE, url.expires, "not base64url!", now));
    }

    #[test]
    fn a_signature_without_an_expiry_signs_the_resource_with_the_key() {
        let signer = UrlSigner::new(b"a household secret");
        let resource = "playbacks/0190f3c4-7d8e-7f90-a1b2-c3d4e5f6a7b8";

        let signature = signer.signature(resource);

        assert!(signer.verify_signature(resource, &signature));
        assert!(!signer.verify_signature(&format!("{resource}0"), &signature));
        assert!(!UrlSigner::new(b"another secret").verify_signature(resource, &signature));
        assert!(!signer.verify_signature(resource, "not base64url!"));
    }
}
