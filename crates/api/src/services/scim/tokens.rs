//! The tokens with which the company's identity provider provisions its
//! accounts through SCIM (ADR-183): made, listed and removed by who manages
//! the company's accounts (`user:manage`, company-wide), whose power such a
//! token holds, as any token another system holds of the company
//! (`services::bearer_tokens`).

use mimorya_db::repositories::bearer_tokens::TokenTable;

use crate::services::bearer_tokens::{MAX_TOKENS, NAME_LENGTH, TokenKind};

/// What a token starts with: told apart from any other secret.
pub const TOKEN_PREFIX: &str = "msc_";

/// The identity provider's tokens.
pub const SCIM_TOKENS: TokenKind = TokenKind {
    table: TokenTable::Scim,
    permission: "user:manage",
    prefix: TOKEN_PREFIX,
    resource: "scim_token",
    created: "scim.token_created",
    deleted: "scim.token_deleted",
    unknown: "Jeton SCIM introuvable",
    too_many,
    no_name,
};

fn too_many() -> String {
    format!("L’entreprise a déjà {MAX_TOKENS} jetons SCIM : retirez-en un d’abord.")
}

fn no_name() -> String {
    format!(
        "Nommez le jeton : le fournisseur d’identité qui le détiendra, en {NAME_LENGTH} \
         caractères au plus."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_refusals_name_the_limits() {
        assert!(too_many().contains(&format!("déjà {MAX_TOKENS} jetons SCIM")));
        assert!(no_name().contains(&format!("{NAME_LENGTH} caractères")));
    }
}
