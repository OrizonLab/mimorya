//! Reads and writes a company's secrets through its tenant transaction.

use std::sync::Arc;

use mimorya_core::DomainError;
use mimorya_db::TenantTx;
use mimorya_db::repositories::secrets::{delete_secret, get_secret, put_secret};
use uuid::Uuid;

use super::SecretCipher;

/// The API key of the company's model provider.
pub const LLM_API_KEY: &str = "llm.api_key";

/// A secret as found: a failure to open it is an answer, not an error (only
/// the database failing is).
pub enum SecretRead {
    Absent,
    Value(String),
    /// Stored, but this server has no master key to open it.
    NoMasterKey,
    /// Stored, but it does not open (other master key, altered row).
    Unreadable,
}

#[derive(Debug, Clone, Default)]
pub struct SecretStore {
    /// `None` when the server has no master key: nothing can be stored.
    cipher: Option<Arc<SecretCipher>>,
}

impl SecretStore {
    pub fn new(cipher: Option<Arc<SecretCipher>>) -> Self {
        Self { cipher }
    }

    pub fn available(&self) -> bool {
        self.cipher.is_some()
    }

    /// Refuses (in French) when nothing can be stored on this server.
    pub fn require(&self) -> Result<(), DomainError> {
        self.cipher().map(|_| ())
    }

    fn cipher(&self) -> Result<&SecretCipher, DomainError> {
        self.cipher.as_deref().ok_or_else(|| {
            DomainError::unavailable(
                "Ce serveur ne peut pas conserver de clé : l’exploitant n’a pas configuré \
                 MIMORYA_MASTER_KEY",
            )
        })
    }

    pub async fn put(
        &self,
        tx: &mut TenantTx,
        tenant_id: Uuid,
        name: &str,
        value: &str,
    ) -> anyhow::Result<()> {
        let sealed = self.cipher()?.seal(tenant_id, name, value)?;
        put_secret(tx, tenant_id, name, &sealed).await?;
        Ok(())
    }

    /// The stored value and whether it opens; an error only when the
    /// database fails.
    pub async fn read(
        &self,
        tx: &mut TenantTx,
        tenant_id: Uuid,
        name: &str,
    ) -> anyhow::Result<SecretRead> {
        let Some(sealed) = get_secret(tx, name).await? else {
            return Ok(SecretRead::Absent);
        };
        let Some(cipher) = self.cipher.as_deref() else {
            return Ok(SecretRead::NoMasterKey);
        };
        Ok(match cipher.open(tenant_id, name, &sealed) {
            Ok(value) => SecretRead::Value(value),
            Err(_) => SecretRead::Unreadable,
        })
    }

    /// Whether a value is stored (without opening it).
    pub async fn exists(&self, tx: &mut TenantTx, name: &str) -> anyhow::Result<bool> {
        Ok(get_secret(tx, name).await?.is_some())
    }

    pub async fn delete(&self, tx: &mut TenantTx, name: &str) -> anyhow::Result<bool> {
        Ok(delete_secret(tx, name).await?)
    }
}
