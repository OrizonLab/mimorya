//! Encryption of the secrets a company stores (model API keys…): AES-256-GCM
//! under the server's master key (`MIMORYA_MASTER_KEY`), with the company and
//! the secret's name as associated data, so that a sealed value only opens
//! in its own place: copied to another company or another name, it fails
//! (ADR-024).

use anyhow::{Context, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use mimorya_db::repositories::secrets::StoredSecret;
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use uuid::Uuid;

pub const MASTER_KEY_LEN: usize = 32;
/// Recorded with each secret: a later key rotation knows which key opens it.
pub const KEY_VERSION: i32 = 1;
const TAG_LEN: usize = 16;

pub struct SecretCipher {
    key: LessSafeKey,
    random: SystemRandom,
}

/// The key never appears in debug output.
impl std::fmt::Debug for SecretCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretCipher(<redacted>)")
    }
}

impl SecretCipher {
    /// From the base64 of 32 random bytes (`openssl rand -base64 32`).
    pub fn from_base64(encoded: &str) -> anyhow::Result<Self> {
        let key = STANDARD
            .decode(encoded.trim())
            .context("master key: base64 expected")?;
        Self::new(&key)
    }

    pub fn new(key: &[u8]) -> anyhow::Result<Self> {
        if key.len() != MASTER_KEY_LEN {
            bail!("master key: {MASTER_KEY_LEN} bytes expected");
        }
        let key = UnboundKey::new(&AES_256_GCM, key).map_err(|_| anyhow!("master key"))?;
        Ok(Self {
            key: LessSafeKey::new(key),
            random: SystemRandom::new(),
        })
    }

    pub fn seal(&self, tenant_id: Uuid, name: &str, value: &str) -> anyhow::Result<StoredSecret> {
        let mut iv = [0u8; NONCE_LEN];
        self.random
            .fill(&mut iv)
            .map_err(|_| anyhow!("no random source for the nonce"))?;
        let mut ciphertext = value.as_bytes().to_vec();
        let tag = self
            .key
            .seal_in_place_separate_tag(
                Nonce::assume_unique_for_key(iv),
                Aad::from(associated_data(tenant_id, name)),
                &mut ciphertext,
            )
            .map_err(|_| anyhow!("sealing failed"))?;
        Ok(StoredSecret {
            ciphertext,
            iv: iv.to_vec(),
            auth_tag: tag.as_ref().to_vec(),
            key_version: KEY_VERSION,
        })
    }

    /// The value, if it was sealed for this company and name by this key and
    /// nothing changed since.
    pub fn open(
        &self,
        tenant_id: Uuid,
        name: &str,
        secret: &StoredSecret,
    ) -> anyhow::Result<String> {
        if secret.key_version != KEY_VERSION {
            bail!("secret sealed by key version {}", secret.key_version);
        }
        if secret.auth_tag.len() != TAG_LEN {
            bail!("malformed secret");
        }
        let nonce = Nonce::try_assume_unique_for_key(&secret.iv)
            .map_err(|_| anyhow!("malformed secret"))?;
        let mut sealed = [secret.ciphertext.as_slice(), secret.auth_tag.as_slice()].concat();
        let value = self
            .key
            .open_in_place(
                nonce,
                Aad::from(associated_data(tenant_id, name)),
                &mut sealed,
            )
            .map_err(|_| anyhow!("secret does not open: other key, place or content"))?;
        String::from_utf8(value.to_vec()).context("secret is not text")
    }
}

fn associated_data(tenant_id: Uuid, name: &str) -> Vec<u8> {
    format!("mimorya-secret/v1/{tenant_id}/{name}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TENANT: Uuid = Uuid::from_u128(1);
    const OTHER: Uuid = Uuid::from_u128(2);

    fn cipher(byte: u8) -> SecretCipher {
        SecretCipher::new(&[byte; MASTER_KEY_LEN]).unwrap()
    }

    #[test]
    fn opens_what_it_sealed_and_never_shows_the_value() {
        let cipher = cipher(7);
        let sealed = cipher.seal(TENANT, "llm.api_key", "sk-ant-secret").unwrap();
        assert!(!sealed.ciphertext.windows(6).any(|w| w == b"secret"));
        assert_eq!(sealed.iv.len(), NONCE_LEN);
        assert_eq!(
            cipher.open(TENANT, "llm.api_key", &sealed).unwrap(),
            "sk-ant-secret"
        );
        // A fresh nonce each time.
        let again = cipher.seal(TENANT, "llm.api_key", "sk-ant-secret").unwrap();
        assert_ne!(again.iv, sealed.iv);
        assert_ne!(again.ciphertext, sealed.ciphertext);
        assert!(!format!("{cipher:?} {sealed:?}").contains("secret\""));
    }

    #[test]
    fn refuses_another_place_another_key_or_a_changed_value() {
        let cipher = cipher(7);
        let sealed = cipher.seal(TENANT, "llm.api_key", "sk-ant-secret").unwrap();
        assert!(cipher.open(OTHER, "llm.api_key", &sealed).is_err());
        assert!(cipher.open(TENANT, "other", &sealed).is_err());
        assert!(
            super::tests::cipher(8)
                .open(TENANT, "llm.api_key", &sealed)
                .is_err()
        );
        let mut flipped = sealed.clone();
        flipped.ciphertext[0] ^= 1;
        assert!(cipher.open(TENANT, "llm.api_key", &flipped).is_err());
        let mut tag = sealed.clone();
        tag.auth_tag[0] ^= 1;
        assert!(cipher.open(TENANT, "llm.api_key", &tag).is_err());
        let mut short = sealed.clone();
        short.iv.pop();
        assert!(cipher.open(TENANT, "llm.api_key", &short).is_err());
        let mut later = sealed;
        later.key_version = 2;
        assert!(cipher.open(TENANT, "llm.api_key", &later).is_err());
    }

    #[test]
    fn reads_the_master_key_from_base64() {
        assert!(SecretCipher::from_base64(&STANDARD.encode([3u8; 32])).is_ok());
        assert!(SecretCipher::from_base64(&STANDARD.encode([3u8; 16])).is_err());
        assert!(SecretCipher::from_base64("pas du base64 !").is_err());
    }
}
