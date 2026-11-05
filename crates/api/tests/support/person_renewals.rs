//! A person's own OAuth authorization of an MCP server as its renewal tests
//! see it (ADR-174): changed in the database as an operator could, its
//! refresh token opened as the server opens it, what the person's account
//! shows of it, and how many renewals the authorization server was asked.

use mimorya_api::services::connectors::person_refresh_sealed_as;
use mimorya_connectors::testing::FakeAuthorizationServer;
use mimorya_core::demo::tenant;
use mimorya_db::repositories::secrets::StoredSecret;
use serde_json::Value;
use uuid::Uuid;

use super::{Client, TestApi};

/// The person's access token expiring in three minutes and a half: due for
/// renewal, still long enough to start a session.
pub const SOON: &str = "access_expires_at = now() + interval '210 seconds'";
/// The person's access token expiring in two minutes: too short to start a
/// session.
pub const EXPIRING: &str = "access_expires_at = now() + interval '2 minutes'";
/// The renewal's claim over, as if its delay had passed.
pub const UNCLAIMED: &str = "renewing_until = null";

/// Sets `set` on `user`'s own authorization of the connector, leaving the
/// live instances alone: a session the person holds stays held.
pub async fn for_person(api: &TestApi, id: &str, user: Uuid, set: &str) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let sql = format!(
        "update connector_person_authorizations set {set} \
         where connector_id = $1 and user_id = $2"
    );
    let changed = sqlx::query(&sql)
        .bind(id.parse::<Uuid>().expect("id"))
        .bind(user)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(changed.rows_affected(), 1, "{sql}");
    tx.commit().await.unwrap();
}

/// A person's refresh token as kept, if any.
#[derive(sqlx::FromRow)]
struct KeptRefresh {
    refresh_ciphertext: Option<Vec<u8>>,
    refresh_iv: Option<Vec<u8>>,
    refresh_auth_tag: Option<Vec<u8>>,
    refresh_key_version: Option<i32>,
}

/// The refresh token the person's own authorization keeps, opened as the
/// server opens it.
pub async fn person_refresh(api: &TestApi, id: &str, user: Uuid) -> Option<String> {
    let connector: Uuid = id.parse().expect("id");
    let services = &api.app.services;
    let mut tx = services.db.begin(tenant::ID).await.unwrap();
    let kept: Option<KeptRefresh> = sqlx::query_as(
        "select refresh_ciphertext, refresh_iv, refresh_auth_tag, refresh_key_version \
         from connector_person_authorizations where connector_id = $1 and user_id = $2",
    )
    .bind(connector)
    .bind(user)
    .fetch_optional(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let kept = kept?;
    let sealed = StoredSecret {
        ciphertext: kept.refresh_ciphertext?,
        iv: kept.refresh_iv?,
        auth_tag: kept.refresh_auth_tag?,
        key_version: kept.refresh_key_version?,
    };
    let cipher = services.config.secrets.clone().expect("master key");
    let name = person_refresh_sealed_as(connector, user);
    Some(
        cipher
            .open(tenant::ID, &name, &sealed)
            .expect("sealed for the person"),
    )
}

/// The person's own authorization of the connector, as their account shows
/// it.
pub async fn own_shown(person: &Client<'_>, id: &str) -> Value {
    let listed = person.get("/account/connector-authorizations").await.json();
    listed["servers"]
        .as_array()
        .and_then(|servers| servers.iter().find(|server| server["connectorId"] == id))
        .map(|server| server["authorization"].clone())
        .unwrap_or(Value::Null)
}

/// How many renewals the authorization server was asked for.
pub fn renewals(authority: &FakeAuthorizationServer) -> usize {
    authority
        .requests()
        .iter()
        .filter(|asked| {
            asked.endpoint == "token"
                && asked.params.get("grant_type").map(String::as_str) == Some("refresh_token")
        })
        .count()
}

/// The system's removals of people's authorizations of this connector:
/// their reasons and whose each was.
pub async fn person_removals(api: &TestApi, id: &str) -> Vec<(String, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<(Option<String>, Value)> = sqlx::query_as(
        "select reason, metadata from audit_log where resource_id = $1 \
         and actor_type = 'system' and action = 'connector.person_oauth_removed' order by id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    rows.into_iter()
        .map(|(reason, metadata)| (reason.unwrap_or_default(), metadata))
        .collect()
}
