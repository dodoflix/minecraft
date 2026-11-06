//! `net.minecraft.util.SignatureValidator` (+ `SignatureUpdater`, flattened to a payload slice: the signed bytes are
//! the concatenation of everything vanilla feeds the updater).

use rsa::RsaPublicKey;
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::pkcs8::DecodePublicKey;
use rsa::signature::Verifier;
use sha2::Sha256;

pub trait SignatureValidator: Send + Sync {
    fn validate(&self, payload: &[u8], signature: &[u8]) -> bool;
}

/// `SignatureValidator.NO_VALIDATION`.
pub struct NoValidation;

impl SignatureValidator for NoValidation {
    fn validate(&self, _payload: &[u8], _signature: &[u8]) -> bool {
        true
    }
}

/// `SignatureValidator.from(publicKey, "SHA256withRSA")`.
pub struct RsaSha256Validator {
    key: VerifyingKey<Sha256>,
}

impl RsaSha256Validator {
    /// `der` is the X.509 SubjectPublicKeyInfo (`ProfilePublicKey.Data.key.getEncoded()`).
    pub fn from_der(der: &[u8]) -> Option<Self> {
        RsaPublicKey::from_public_key_der(der).ok().map(|key| Self {
            key: VerifyingKey::new(key),
        })
    }
}

impl SignatureValidator for RsaSha256Validator {
    fn validate(&self, payload: &[u8], signature: &[u8]) -> bool {
        match Signature::try_from(signature) {
            Ok(sig) => self.key.verify(payload, &sig).is_ok(),
            Err(e) => {
                // vanilla: `Signature.verify` throws `SignatureException` for a malformed signature -> logged, false
                log::error!("Failed to verify signature: {e}");
                false
            }
        }
    }
}
