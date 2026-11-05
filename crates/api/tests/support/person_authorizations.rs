//! Each person's OAuth authorization of an MCP server in the tests
//! (ADR-174): one kept as the platform keeps it, its token issued by the
//! simulated authorization server and sealed for that person, and what the
//! database and the audit keep of them.

use mimorya_api::services::connectors::person_access_sealed_as;
use mimorya_connectors::testing::{CLIENT_ID, FakeAuthorizationServer};
use mimorya_core::demo::tenant;
use mimorya_db::repositories::connector_person_authorizations::{
    NewPersonAuthorization, count_person_authorizations, put_person_authorization,
};
use serde_json::Value;
use uuid::Uuid;

use super::TestApi;

/// Keeps `user`'s authorization of the connector, as if they had given it
/// under its declaration (`resource`): the access token issued, which does
/// not expire.
pub async fn person_authorized(
    api: &TestApi,
    id: &str,
    user: Uuid,
    authority: &FakeAuthorizationServer,
    resource: &str,
) -> String {
    let connector: Uuid = id.parse().expect("id");
    let token = authority.issue(resource);
    let services = &api.app.services;
    let cipher = services.config.secrets.clone().expect("master key");
    let sealed = cipher
        .seal(
            tenant::ID,
            &person_access_sealed_as(connector, user),
            &token,
        )
        .expect("sealed");
    let mut tx = services.db.begin(tenant::ID).await.unwrap();
    put_person_authorization(
        &mut tx,
        NewPersonAuthorization {
            tenant_id: tenant::ID,
            connector_id: connector,
            user_id: user,
            issuer: &authority.issuer,
            client_id: CLIENT_ID,
            resource,
            scopes: "",
            access_token: &sealed,
            access_expires_at: None,
            refresh_token: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    token
}

/// How many people's authorizations the connector keeps.
pub async fn person_count(api: &TestApi, id: &str) -> i64 {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let count = count_person_authorizations(&mut tx, id.parse().expect("id"))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    count
}

/// The audited revocations of the connector's people's authorizations:
/// outcome, reason and counts.
pub async fn person_revocations(api: &TestApi, id: &str) -> Vec<(String, Option<String>, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows = sqlx::query_as(
        "select outcome, reason, metadata from audit_log where resource_id = $1 \
         and action = 'connector.oauth_person_authorizations_revoked' order by id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    rows
}

/// The metadata of the connector's last audit entry `action`.
pub async fn last_metadata(api: &TestApi, id: &str, action: &str) -> Value {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let metadata = sqlx::query_scalar(
        "select metadata from audit_log where resource_id = $1 and action = $2 \
         order by id desc limit 1",
    )
    .bind(id)
    .bind(action)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    metadata
}
