use crate::codec::EncodeResult;
use rand_core::{OsRng, RngCore};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePublicKey};
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey, traits::PublicKeyParts};
use sha1::{Digest, Sha1};

use crate::codec::{Decode, DecodeError, DecodeResult, Encode};

/// Largest key (DER bytes) vanilla accepts on the wire: `ByteBufCodecs.MAX_PUBLIC_KEY_LENGTH`.
const MAX_PUBLIC_KEY_LENGTH: usize = 512;

#[derive(Debug, thiserror::Error)]
pub enum CryptError {
    #[error("RSA: {0}")]
    Rsa(#[from] rsa::Error),
    #[error("malformed key: {0}")]
    Key(String),
    /// Vanilla `Crypt.decryptByteToSecretKey` accepts any non-empty length (`SecretKeySpec`), but the
    /// following `Crypt.getCipher` (AES/CFB8, IV = key bytes) only succeeds for exactly 16 bytes
    /// (AES-192/256 keys fail on the 24/32-byte IV), which `handleKey` reports as "Protocol error".
    /// An empty key already fails in `SecretKeySpec`. All of these are errors here.
    #[error("invalid AES secret key length {0}, expected 16")]
    SecretKeyLength(usize),
}

/// RSA-1024 server key (`Crypt.generateKeyPair`).
pub struct ServerKeyPair {
    private: RsaPrivateKey,
    public_der: Vec<u8>,
}

impl ServerKeyPair {
    pub fn generate() -> Result<Self, CryptError> {
        Self::from_private(RsaPrivateKey::new(&mut OsRng, 1024)?)
    }

    /// Loads a PKCS#8 DER private key (Java `PrivateKey.getEncoded()`).
    pub fn from_pkcs8_der(der: &[u8]) -> Result<Self, CryptError> {
        Self::from_private(RsaPrivateKey::from_pkcs8_der(der).map_err(|e| CryptError::Key(e.to_string()))?)
    }

    fn from_private(private: RsaPrivateKey) -> Result<Self, CryptError> {
        let public_der = RsaPublicKey::from(&private)
            .to_public_key_der()
            .map_err(|e| CryptError::Key(e.to_string()))?
            .into_vec();
        Ok(Self { private, public_der })
    }

    /// X.509 SubjectPublicKeyInfo, exactly the bytes of `ClientboundHelloPacket.publicKey`.
    pub fn public_key_der(&self) -> &[u8] {
        &self.public_der
    }

    /// `Crypt.decryptUsingKey`: RSA/ECB/PKCS1Padding.
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptError> {
        let k = self.private.size();
        if ciphertext.len() > k {
            return Err(rsa::Error::Decryption.into());
        }
        // Java's RSA cipher interprets the input as an integer, so a ciphertext shorter than the
        // modulus (stripped leading zeros) is still valid.
        let mut padded = vec![0u8; k];
        padded[k - ciphertext.len()..].copy_from_slice(ciphertext);
        Ok(self.private.decrypt_blinded(&mut OsRng, Pkcs1v15Encrypt, &padded)?)
    }

    /// `Crypt.decryptByteToSecretKey` followed by the AES init of `Crypt.getCipher`; see
    /// [`CryptError::SecretKeyLength`].
    pub fn decrypt_secret_key(&self, ciphertext: &[u8]) -> Result<[u8; 16], CryptError> {
        let key = self.decrypt(ciphertext)?;
        <[u8; 16]>::try_from(key.as_slice()).map_err(|_| CryptError::SecretKeyLength(key.len()))
    }
}

/// RSA public key as sent on the wire (`ByteBufCodecs.PUBLIC_KEY`): the key plus its X.509 DER.
#[derive(Clone, PartialEq, Eq)]
pub struct PublicKey {
    key: RsaPublicKey,
    der: Vec<u8>,
}

impl std::fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PublicKey(RSA-{})", self.key.n().bits())
    }
}

impl PublicKey {
    pub fn der(&self) -> &[u8] {
        &self.der
    }

    /// `Crypt.encryptUsingKey`: RSA/ECB/PKCS1Padding.
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>, CryptError> {
        Ok(self.key.encrypt(&mut OsRng, Pkcs1v15Encrypt, data)?)
    }
}

/// `Crypt.byteToPublicKey`.
pub fn public_key_from_der(der: &[u8]) -> Result<PublicKey, CryptError> {
    let key = RsaPublicKey::from_public_key_der(der).map_err(|e| CryptError::Key(e.to_string()))?;
    Ok(PublicKey { key, der: der.to_vec() })
}

/// `Crypt.encryptUsingKey` with a DER-encoded public key.
pub fn encrypt_with_public_key(public_key_der: &[u8], data: &[u8]) -> Result<Vec<u8>, CryptError> {
    public_key_from_der(public_key_der)?.encrypt(data)
}

/// `Crypt.generateSecretKey`: a random AES-128 key.
pub fn generate_secret() -> [u8; 16] {
    let mut key = [0u8; 16];
    OsRng.fill_bytes(&mut key);
    key
}

impl Encode for PublicKey {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        // `ByteBufCodecs.PUBLIC_KEY = byteArray(512).map(.., Key::getEncoded)`
        <u8 as Encode>::encode_vec_bounded(&self.der, out, MAX_PUBLIC_KEY_LENGTH)
    }
}

impl Decode for PublicKey {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let der = <u8 as Decode>::decode_vec(input, MAX_PUBLIC_KEY_LENGTH)?;
        public_key_from_der(&der).map_err(|_| DecodeError::msg("Malformed public key bytes"))
    }
}

/// `new BigInteger(Crypt.digestData(server_id, key, secret)).toString(16)`: SHA-1 printed as a
/// signed two's-complement number (leading `-` when negative, no leading zeros).
pub fn digest_server_id(server_id: &str, shared_secret: &[u8], public_key_der: &[u8]) -> String {
    // `String.getBytes("ISO_8859_1")`: unmappable characters (one per code point) become '?'.
    let id: Vec<u8> = server_id.chars().map(|c| u8::try_from(c as u32).unwrap_or(b'?')).collect();
    let mut digest: [u8; 20] = Sha1::new()
        .chain_update(&id)
        .chain_update(shared_secret)
        .chain_update(public_key_der)
        .finalize()
        .into();

    let negative = digest[0] & 0x80 != 0;
    if negative {
        // Two's complement negation to get the magnitude.
        let mut carry = true;
        for b in digest.iter_mut().rev() {
            *b = !*b;
            if carry {
                (*b, carry) = b.overflowing_add(1);
            }
        }
    }
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    let hex = hex.trim_start_matches('0');
    match (negative, hex.is_empty()) {
        (_, true) => "0".into(),
        (true, false) => format!("-{hex}"),
        (false, false) => hex.into(),
    }
}
