//! Online-mode login: RSA/AES helpers (`net.minecraft.util.Crypt`), the session-server
//! `hasJoined` call (authlib `MinecraftServicesSessionService`) and the chat-session
//! `ByteBufCodecs.PUBLIC_KEY` wire codec.

mod crypt;
mod session;

pub use crypt::{
    CryptError, PublicKey, ServerKeyPair, digest_server_id, encrypt_with_public_key,
    generate_secret, public_key_from_der,
};
pub use session::{AuthError, SessionClient};
