//! Whether the account a provider proved may sign in: active (ADR-063), or
//! invited, whose invitation the sign-in accepts (ADR-064). The provider
//! vouches for the address the invitation was sent to: the account becomes
//! active without a password, and its invitation link is spent.

use mimorya_db::repositories::auth::lock_login_state;
use mimorya_db::repositories::invitations::delete_invitation_of;
use mimorya_db::repositories::users::activate_invited_without_password;
use sqlx::PgConnection;
use uuid::Uuid;

pub enum Admission {
    Active,
    /// The invitation was accepted by this sign-in.
    Accepted,
}

/// `None` when the account may not sign in. Its row is locked first, as a
/// password sign-in does: a disabling committed meanwhile wins.
pub async fn admit(conn: &mut PgConnection, user_id: Uuid) -> sqlx::Result<Option<Admission>> {
    let status = lock_login_state(&mut *conn, user_id)
        .await?
        .map(|(_, status)| status);
    match status.as_deref() {
        Some("active") => Ok(Some(Admission::Active)),
        Some("invited") => {
            if !activate_invited_without_password(&mut *conn, user_id).await? {
                return Ok(None);
            }
            delete_invitation_of(conn, user_id).await?;
            Ok(Some(Admission::Accepted))
        }
        _ => Ok(None),
    }
}
