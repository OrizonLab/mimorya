//! SCIM 2.0 as the company's identity provider reads and writes it (RFC
//! 7643, RFC 7644, ADR-183): an account, a page of accounts, a refusal and
//! what this server supports. Its names are SCIM's (`userName`,
//! `Resources`…), not the application's.

use mimorya_db::repositories::scim_users::ScimUserRow;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::http::iso;

pub const USER_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
pub const LIST_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";
pub const ERROR_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:Error";
pub const CONFIG_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig";

/// The most accounts a page holds, whatever the provider asks.
pub const MAX_RESULTS: i64 = 200;

/// Where the provider reaches SCIM, from the application's address.
pub fn scim_url(web_origin: &str) -> String {
    format!("{}/api/scim/v2", web_origin.trim_end_matches('/'))
}

/// An account, as the provider reads it.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScimUser {
    pub schemas: Vec<String>,
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub external_id: Option<String>,
    /// The account's address.
    pub user_name: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    pub emails: Vec<ScimEmail>,
    /// Neither disabled nor removed: an account still invited is active.
    pub active: bool,
    pub meta: ScimMeta,
}

#[derive(Debug, Serialize, TS)]
pub struct ScimEmail {
    pub value: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub primary: bool,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScimMeta {
    pub resource_type: String,
    pub created: String,
    pub last_modified: String,
    pub location: String,
}

impl ScimUser {
    /// The account `row`, as the provider reaching SCIM at `base` reads it.
    pub fn of(row: &ScimUserRow, base: &str) -> Self {
        Self {
            schemas: vec![USER_SCHEMA.into()],
            id: row.id,
            external_id: row.external_id.clone(),
            user_name: row.email.clone(),
            display_name: row.display_name.clone(),
            title: row.job_title.clone(),
            emails: vec![ScimEmail {
                value: row.email.clone(),
                kind: "work".into(),
                primary: true,
            }],
            active: row.status != "disabled",
            meta: ScimMeta {
                resource_type: "User".into(),
                created: iso(&row.created_at),
                last_modified: iso(&row.updated_at),
                location: format!("{base}/Users/{}", row.id),
            },
        }
    }
}

/// A page of accounts (`GET /scim/v2/Users`).
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScimUserList {
    pub schemas: Vec<String>,
    /// Every account the request matches, on this page or not.
    pub total_results: i64,
    /// The place of the page's first account, from 1.
    pub start_index: i64,
    pub items_per_page: i64,
    #[serde(rename = "Resources")]
    pub resources: Vec<ScimUser>,
}

/// A refusal, as SCIM writes it.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScimErrorBody {
    pub schemas: Vec<String>,
    /// The HTTP status, as a string.
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scim_type: Option<String>,
    pub detail: String,
}

/// What this server supports (`GET /scim/v2/ServiceProviderConfig`).
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScimServiceProviderConfig {
    pub schemas: Vec<String>,
    pub patch: ScimSupported,
    pub bulk: ScimBulk,
    pub filter: ScimFilterSupport,
    pub change_password: ScimSupported,
    pub sort: ScimSupported,
    pub etag: ScimSupported,
    pub authentication_schemes: Vec<ScimAuthenticationScheme>,
    pub meta: ScimConfigMeta,
}

#[derive(Debug, Serialize, TS)]
pub struct ScimSupported {
    pub supported: bool,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScimBulk {
    pub supported: bool,
    pub max_operations: i64,
    pub max_payload_size: i64,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScimFilterSupport {
    pub supported: bool,
    pub max_results: i64,
}

#[derive(Debug, Serialize, TS)]
pub struct ScimAuthenticationScheme {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub description: String,
    pub primary: bool,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScimConfigMeta {
    pub resource_type: String,
    pub location: String,
}

impl ScimServiceProviderConfig {
    /// What this server supports, reached at `base`: filters on `userName`
    /// and `externalId`, nothing of patch, bulk, sort, ETags or passwords.
    pub fn at(base: &str) -> Self {
        let no = || ScimSupported { supported: false };
        Self {
            schemas: vec![CONFIG_SCHEMA.into()],
            patch: no(),
            bulk: ScimBulk {
                supported: false,
                max_operations: 0,
                max_payload_size: 0,
            },
            filter: ScimFilterSupport {
                supported: true,
                max_results: MAX_RESULTS,
            },
            change_password: no(),
            sort: no(),
            etag: no(),
            authentication_schemes: vec![ScimAuthenticationScheme {
                kind: "oauthbearertoken".into(),
                name: "Jeton SCIM".into(),
                description: "Jeton SCIM de l’entreprise, fait dans son administration, \
                              envoyé dans l’en-tête Authorization: Bearer"
                    .into(),
                primary: true,
            }],
            meta: ScimConfigMeta {
                resource_type: "ServiceProviderConfig".into(),
                location: format!("{base}/ServiceProviderConfig"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn writes_an_account_as_scim_names_it() {
        let at = Utc.with_ymd_and_hms(2026, 11, 7, 9, 30, 0).unwrap();
        let row = ScimUserRow {
            id: Uuid::nil(),
            email: "lea.martin@neo.example".into(),
            display_name: "Léa Martin".into(),
            job_title: None,
            status: "invited".into(),
            external_id: Some("a1".into()),
            created_at: at,
            updated_at: at,
        };
        let user =
            serde_json::to_value(ScimUser::of(&row, &scim_url("https://m.example/"))).unwrap();
        assert_eq!(user["userName"], "lea.martin@neo.example");
        assert_eq!(user["externalId"], "a1");
        assert_eq!(user["active"], true);
        assert!(user.get("title").is_none());
        assert_eq!(user["emails"][0]["type"], "work");
        assert_eq!(
            user["meta"]["location"],
            format!("https://m.example/api/scim/v2/Users/{}", Uuid::nil())
        );
        let disabled = ScimUserRow {
            status: "disabled".into(),
            ..row
        };
        assert!(!ScimUser::of(&disabled, "").active);
    }
}
