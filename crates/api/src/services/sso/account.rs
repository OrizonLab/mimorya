//! The account a proven person signs in to (ADR-063): the one tied to their
//! subject at this provider; at their first sign-in there, the company's
//! account with the address the provider vouches for, in one of its
//! domains; failing that, when the provider creates accounts and the
//! sign-in started from the company's link (ADR-067), a new one: active,
//! without a password, a role or a unit, named as the provider names the
//! person. A provider that cannot prove its domains creates it only in a
//! domain the company proved, and none creates it in a domain another
//! company proved (ADR-069).

use mimorya_core::identity_providers::{ProvenIdentity, ProviderKind, email_domain};
use mimorya_db::repositories::identity_providers::IdentityProviderRow;
use mimorya_db::repositories::user_identities::{
    find_identity, find_user_by_email, has_identity_at, touch_identity,
};
use mimorya_db::repositories::users::create_federated_user;
use mimorya_db::repositories::verified_domains::{domain_held_elsewhere, is_domain_verified};
use sqlx::PgConnection;
use uuid::Uuid;

use super::sign_in::SignInFailure;

pub struct Account {
    pub user_id: Uuid,
    /// Already tied to the person's subject at this provider.
    pub tied: bool,
    /// Created by this sign-in.
    pub created: bool,
}

/// `name` is the account's name when this sign-in may create it: started
/// from the company's link, never from an address.
pub async fn account_of(
    conn: &mut PgConnection,
    provider: &IdentityProviderRow,
    proven: &ProvenIdentity,
    name: Option<&str>,
) -> Result<Account, SignInFailure> {
    let at = |reason: &str| SignInFailure::at(provider.tenant_id, provider.id, reason);
    let internal = |e: sqlx::Error| SignInFailure::internal(provider.tenant_id, provider.id, e);
    if let Some(tie) = find_identity(&mut *conn, provider.id, &proven.subject)
        .await
        .map_err(internal)?
    {
        touch_identity(&mut *conn, tie.id).await.map_err(internal)?;
        return Ok(Account {
            user_id: tie.user_id,
            tied: true,
            created: false,
        });
    }
    let email = proven
        .email
        .as_deref()
        .ok_or_else(|| at("Adresse e-mail non garantie par le fournisseur"))?;
    let Some(domain) = email_domain(email).filter(|d| provider.domains.contains(d)) else {
        return Err(at("Adresse hors des domaines du fournisseur"));
    };
    if let Some(user_id) = find_user_by_email(&mut *conn, email)
        .await
        .map_err(internal)?
    {
        if has_identity_at(&mut *conn, provider.id, user_id)
            .await
            .map_err(internal)?
        {
            return Err(at("Compte déjà relié à une autre identité de ce fournisseur").of(user_id));
        }
        return Ok(Account {
            user_id,
            tied: false,
            created: false,
        });
    }
    let Some(name) = name.filter(|_| provider.create_accounts) else {
        return Err(at("Aucun compte de l’entreprise pour cette adresse"));
    };
    // Its address is unique across companies: taken here, it would be taken
    // from the company that holds the domain.
    let proves = ProviderKind::parse(&provider.kind).is_some_and(ProviderKind::proves_its_domains);
    if !proves
        && !is_domain_verified(&mut *conn, &domain)
            .await
            .map_err(internal)?
    {
        return Err(at(
            "Domaine non prouvé par l’entreprise : aucun compte n’y est créé",
        ));
    }
    if domain_held_elsewhere(&mut *conn, provider.tenant_id, &domain)
        .await
        .map_err(internal)?
    {
        return Err(at("Adresse d’un domaine prouvé par une autre entreprise"));
    }
    // Taken in another company (addresses are unique across companies,
    // ADR-011), or created meanwhile by another sign-in of the person.
    let user_id = create_federated_user(conn, provider.tenant_id, email, name)
        .await
        .map_err(internal)?
        .ok_or_else(|| at("Adresse déjà utilisée par un autre compte"))?;
    Ok(Account {
        user_id,
        tied: false,
        created: true,
    })
}
