//! Documents deposited through the API (ADR-095), replaced by a new version
//! (ADR-150), and what the database keeps of them.

use axum::http::StatusCode;
use mimorya_core::demo::tenant;
use serde_json::Value;
use uuid::Uuid;

use super::{Client, TestApi, TestResponse};

/// Where a deposit goes and how sensitive it is: the company's space
/// without a unit, `internal` without a sensitivity.
#[derive(Debug, Clone, Copy, Default)]
pub struct Target<'a> {
    pub unit: Option<Uuid>,
    pub sensitivity: Option<&'a str>,
}

impl<'a> Target<'a> {
    pub fn unit(unit: Uuid) -> Self {
        Self {
            unit: Some(unit),
            sensitivity: None,
        }
    }

    pub fn sensitive(self, sensitivity: &'a str) -> Self {
        Self {
            sensitivity: Some(sensitivity),
            ..self
        }
    }
}

/// The deposit URL of the file `name` into `into`.
pub fn deposit_url(name: &str, into: Target<'_>) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair("name", name);
    if let Some(unit) = into.unit {
        query.append_pair("unitId", &unit.to_string());
    }
    if let Some(sensitivity) = into.sensitivity {
        query.append_pair("sensitivity", sensitivity);
    }
    format!("/documents?{}", query.finish())
}

/// Deposits `bytes` as the file `name` into `into`.
pub async fn deposit(
    client: &Client<'_>,
    name: &str,
    into: Target<'_>,
    bytes: &[u8],
) -> TestResponse {
    let url = deposit_url(name, into);
    client
        .post_file(&url, bytes.to_vec(), "application/octet-stream")
        .await
}

/// Deposits `bytes` as the file `name` into `into`, taken: its identifier.
pub async fn deposited(client: &Client<'_>, name: &str, into: Target<'_>, bytes: &[u8]) -> Uuid {
    let taken = deposit(client, name, into, bytes).await;
    assert_eq!(taken.status, StatusCode::CREATED, "{}", taken.text());
    taken.json()["id"].as_str().unwrap().parse().unwrap()
}

/// The URL replacing the file of the document `id`, at `version`, by the
/// file `name`.
pub fn replace_url(id: Uuid, name: &str, version: i32) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair("name", name);
    query.append_pair("version", &version.to_string());
    format!("/documents/{id}/file?{}", query.finish())
}

/// Replaces the file of the document `id`, at `version`, by `bytes` named
/// `name`.
pub async fn replace(
    client: &Client<'_>,
    id: Uuid,
    name: &str,
    version: i32,
    bytes: &[u8],
) -> TestResponse {
    let url = replace_url(id, name, version);
    client
        .put_file(&url, bytes.to_vec(), "application/octet-stream")
        .await
}

/// Deletes the document `id`, as `client` may.
pub async fn remove(client: &Client<'_>, id: Uuid) {
    let deleted = client.delete(&format!("/documents/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
}

/// The identifiers of the documents `client` reads.
pub async fn listed(client: &Client<'_>) -> Vec<Uuid> {
    let list = client.get("/documents").await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.text());
    list.json()["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|document| document["id"].as_str().unwrap().parse().unwrap())
        .collect()
}

/// The passages kept of the document `id`: page, title, text.
pub async fn passages_of(api: &TestApi, id: Uuid) -> Vec<(Option<i32>, Option<String>, String)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select page, heading, content from document_chunks where document_id = $1 \
         order by ordinal",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

/// How many documents, files and passages the demo company keeps.
pub async fn kept(api: &TestApi) -> (i64, i64, i64) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select (select count(*) from documents), (select count(*) from document_files), \
         (select count(*) from document_chunks)",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap()
}

/// The audit entries of `action`: their resource and metadata.
pub async fn audited(api: &TestApi, action: &str) -> Vec<(Option<String>, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as("select resource_id, metadata from audit_log where action = $1 order by id")
        .bind(action)
        .fetch_all(&mut *tx)
        .await
        .unwrap()
}
