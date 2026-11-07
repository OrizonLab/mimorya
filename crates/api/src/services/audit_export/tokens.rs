//! The tokens with which the company's SIEM reads its audit log (ADR-181):
//! made, listed and removed by who may export the log on the whole company
//! (`audit:export`), as any token another system holds of the company
//! (`services::bearer_tokens`).

use mimorya_db::Database;
use mimorya_db::repositories::bearer_tokens::{BearerTokenRow, TokenTable};
use uuid::Uuid;

use crate::services::AccessContext;
use crate::services::bearer_tokens::{MAX_TOKENS, NAME_LENGTH, TokenKind};

/// What a token starts with: told apart from any other secret.
pub const TOKEN_PREFIX: &str = "mae_";

/// The SIEM's tokens.
pub(super) const EXPORT_TOKENS: TokenKind = TokenKind {
    table: TokenTable::AuditExport,
    permission: "audit:export",
    prefix: TOKEN_PREFIX,
    resource: "audit_export_token",
    created: "audit.export_token_created",
    deleted: "audit.export_token_deleted",
    unknown: "Jeton d’export introuvable",
    too_many,
    no_name,
};

fn too_many() -> String {
    format!("L’entreprise a déjà {MAX_TOKENS} jetons d’export : retirez-en un d’abord.")
}

fn no_name() -> String {
    format!("Nommez le jeton : le SIEM qui le détiendra, en {NAME_LENGTH} caractères au plus.")
}

/// The company's tokens, never their secret.
pub async fn audit_export_tokens(
    db: &Database,
    access: &AccessContext,
) -> anyhow::Result<Vec<BearerTokenRow>> {
    EXPORT_TOKENS.list(db, access).await
}

/// A new token named `name`, and its secret, shown this once.
pub async fn create_audit_export_token(
    db: &Database,
    access: &AccessContext,
    name: &str,
) -> anyhow::Result<(BearerTokenRow, String)> {
    EXPORT_TOKENS.create(db, access, name).await
}

/// Removes the company's token `id`: its SIEM reads nothing more with it.
pub async fn delete_audit_export_token(
    db: &Database,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    EXPORT_TOKENS.delete(db, access, id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_refusals_name_the_limits() {
        assert!(too_many().contains(&format!("déjà {MAX_TOKENS} jetons")));
        assert!(no_name().contains(&format!("{NAME_LENGTH} caractères")));
    }
}
