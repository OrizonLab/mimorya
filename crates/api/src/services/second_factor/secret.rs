//! What a second factor is made of (ADR-200): its authenticator secret,
//! drawn at random and sealed under the server's master key for this
//! account alone, and its recovery codes, drawn at random, shown once and
//! kept as digests.

use anyhow::anyhow;
use argon2::password_hash::rand_core::{OsRng, RngCore};
use mimorya_core::second_factor::{
    CODE_BYTES, RECOVERY_CODES, SECRET_LEN, base32, read_base32, recovery_code, recovery_digest,
};
use mimorya_db::repositories::secrets::StoredSecret;
use uuid::Uuid;

use crate::secrets::SecretCipher;

/// The name a secret is sealed under: it opens for this account and
/// company only.
fn sealed_name(user_id: Uuid) -> String {
    format!("second-factor/{user_id}")
}

/// A new secret, as its authenticator app is given it (base32), sealed.
pub(super) fn new_secret(
    cipher: &SecretCipher,
    tenant_id: Uuid,
    user_id: Uuid,
) -> anyhow::Result<(Vec<u8>, StoredSecret)> {
    let mut secret = vec![0u8; SECRET_LEN];
    OsRng.fill_bytes(&mut secret);
    let sealed = cipher.seal(tenant_id, &sealed_name(user_id), &base32(&secret))?;
    Ok((secret, sealed))
}

/// The secret `sealed` holds, if it opens for this account.
pub(super) fn open_secret(
    cipher: &SecretCipher,
    tenant_id: Uuid,
    user_id: Uuid,
    sealed: &StoredSecret,
) -> anyhow::Result<Vec<u8>> {
    let text = cipher.open(tenant_id, &sealed_name(user_id), sealed)?;
    read_base32(&text).ok_or_else(|| anyhow!("second factor secret is not base32"))
}

/// New recovery codes, as shown, with what is kept of them.
pub(super) fn new_recovery_codes() -> (Vec<String>, Vec<Vec<u8>>) {
    (0..RECOVERY_CODES)
        .map(|_| {
            let mut bytes = [0u8; CODE_BYTES];
            OsRng.fill_bytes(&mut bytes);
            // As a typed code is read: its sixteen characters, no dashes.
            (recovery_code(&bytes), recovery_digest(&base32(&bytes)))
        })
        .unzip()
}

#[cfg(test)]
mod tests {
    use mimorya_core::second_factor::read_recovery_code;

    use super::*;

    #[test]
    fn a_secret_opens_for_its_account_only() {
        let cipher = SecretCipher::new(&[7; 32]).unwrap();
        let (tenant, user) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let (secret, sealed) = new_secret(&cipher, tenant, user).unwrap();
        assert_eq!(secret.len(), SECRET_LEN);
        assert_eq!(open_secret(&cipher, tenant, user, &sealed).unwrap(), secret);
        assert!(open_secret(&cipher, tenant, Uuid::from_u128(3), &sealed).is_err());
        assert!(open_secret(&cipher, Uuid::from_u128(3), user, &sealed).is_err());
    }

    #[test]
    fn recovery_codes_are_distinct_and_kept_as_their_digest() {
        let (shown, kept) = new_recovery_codes();
        assert_eq!(shown.len(), RECOVERY_CODES);
        assert_eq!(kept.len(), RECOVERY_CODES);
        for (code, digest) in shown.iter().zip(&kept) {
            assert_eq!(digest.len(), 32);
            let read = read_recovery_code(code).unwrap();
            assert_eq!(&recovery_digest(&read), digest);
        }
        let distinct: std::collections::BTreeSet<_> = shown.iter().collect();
        assert_eq!(distinct.len(), RECOVERY_CODES);
    }
}
