//! Another company with its own Slack gateway, for the tests that a company
//! reaches nothing of another's (ADR-125, ADR-132): its administrator, her
//! invitation accepted, declares it with the local Slack's application and
//! her company's own signing secret; or its own Telegram bot (ADR-133).

use mimorya_api::services::admin::{NewSlackGatewayInput, declare_slack_gateway};
use mimorya_api::services::{AccessContext, AuthIdentity};
use mimorya_connectors::testing::BOT_TOKEN;
use uuid::Uuid;

use super::TestApi;
use super::companies;
use super::queue::as_owner;

/// Another company's own signing secret.
pub const OTHER_SECRET: &str = "0ther5ecret0ther5ecret0ther5ecr";

/// Another company, and its Slack gateway.
pub async fn other_company(api: &TestApi, slug: &str) -> (Uuid, Uuid) {
    let (tenant_id, gateway, _) = other_company_with_admin(api, slug).await;
    (tenant_id, gateway)
}

/// Another company, its Slack gateway, and its administrator's rights.
pub async fn other_company_with_admin(api: &TestApi, slug: &str) -> (Uuid, Uuid, AccessContext) {
    let (tenant_id, jeanne) = other_administrator(api, slug).await;
    let input = NewSlackGatewayInput {
        name: "Slack Acme".into(),
        bot_token: BOT_TOKEN.into(),
        signing_secret: OTHER_SECRET.into(),
        enabled: true,
    };
    let gateway = declare_slack_gateway(&api.app.services.gateways, &jeanne, input)
        .await
        .unwrap();
    (tenant_id, gateway, jeanne)
}

/// Another company, and its administrator's rights, her invitation
/// accepted.
pub async fn other_administrator(api: &TestApi, slug: &str) -> (Uuid, AccessContext) {
    let acme = companies::create(api, slug, &format!("jeanne.martin@{slug}.example")).await;
    as_owner(
        "update users set status = 'active' where id = $1",
        acme.admin_id,
    )
    .await;
    let jeanne: AccessContext = api
        .app
        .services
        .access
        .context(AuthIdentity {
            tenant_id: acme.tenant_id,
            user_id: acme.admin_id,
            session_id: Uuid::nil(),
            must_change_password: false,
        })
        .await
        .unwrap();
    (acme.tenant_id, jeanne)
}
