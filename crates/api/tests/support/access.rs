//! What the authorization engine decides for a demo user, as a request would
//! see it (fresh subject, cached company engine).

use mimorya_api::services::{AccessContext, AuthIdentity};
use mimorya_core::authz::{Actor, ResourceRef};
use mimorya_core::demo::tenant;
use uuid::Uuid;

use super::TestApi;

pub async fn context_of(api: &TestApi, user_id: Uuid) -> AccessContext {
    let identity = AuthIdentity {
        tenant_id: tenant::ID,
        user_id,
        session_id: Uuid::nil(),
        must_change_password: false,
    };
    api.app.services.access.context(identity).await.unwrap()
}

/// Whether the user may read an intervention they own, in this status.
pub async fn reads(api: &TestApi, user_id: Uuid, status: &str) -> bool {
    let access = context_of(api, user_id).await;
    let intervention = ResourceRef::new("intervention", tenant::ID)
        .owner(Some(user_id))
        .attribute("status", status);
    access
        .engine
        .check(
            &access.subject,
            "intervention:read",
            &intervention,
            &Actor::user(),
        )
        .allowed
}
