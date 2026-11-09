//! What an erasure refuses (ADR-189), each refusal leaving the account as
//! it was. Hugo is erased only once disabled, with his own address typed
//! again, none of his runs still living, no request awaiting his decision,
//! standing in for nobody and leading no unit. Nobody erases their own account; managing accounts is not
//! enough to erase one; another company's administrator finds no such
//! account, nor does anyone an account that never was.

mod support;

use mimorya_core::demo::{tenant, unit, user};
use serde_json::json;
use support::accounts::{INVITED_PASSWORD, accept, holder_of};
use support::erasure::exists;
use support::idle_conversations::conversation;
use support::personal_export::absence;
use support::queue::{API, as_owner};
use support::{Client, TestApi, companies, email};
use uuid::Uuid;

/// `who` erases `person`, typing `typed`.
async fn erase(who: &Client<'_>, person: Uuid, typed: &str) -> support::TestResponse {
    who.post(
        &format!("/admin/users/{person}/erasure"),
        json!({ "confirmEmail": typed }),
    )
    .await
}

/// Refused with `status` and `message`; the account still there.
async fn refused(response: support::TestResponse, status: u16, message: &str, person: Uuid) {
    assert_eq!(response.status.as_u16(), status, "{}", response.text());
    assert!(
        response.error_message().contains(message),
        "{}",
        response.text()
    );
    assert!(exists(person).await);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_is_erased_once_nothing_ties_it_to_others() {
    let api = TestApi::start_with(API).await;
    let paul = api.sign_in(user::PAUL).await;
    let hugo = email(user::HUGO);

    // Active: disabled first.
    refused(
        erase(&paul, user::HUGO, &hugo).await,
        409,
        "Désactivez",
        user::HUGO,
    )
    .await;
    let disabled = paul
        .patch(
            &format!("/admin/users/{}", user::HUGO),
            json!({ "status": "disabled" }),
        )
        .await;
    assert_eq!(disabled.status.as_u16(), 200, "{}", disabled.text());

    // Another address, or none.
    for typed in [email(user::MARC), String::new()] {
        let response = erase(&paul, user::HUGO, &typed).await;
        assert_eq!(response.status.as_u16(), 400, "{}", response.text());
        assert!(exists(user::HUGO).await);
    }

    // A run of his waiting for a validation.
    let talk = conversation(tenant::ID, user::HUGO, "Demande de Hugo", 1).await;
    let waiting = "insert into agent_runs (tenant_id, conversation_id, on_behalf_of_user_id, \
                   objective, budget, status) select tenant_id, id, owner_user_id, \
                   'objectif', '{}', 'awaiting_approval' from conversations where id = $1";
    as_owner(waiting, talk).await;
    refused(
        erase(&paul, user::HUGO, &hugo).await,
        409,
        "en cours",
        user::HUGO,
    )
    .await;
    as_owner(
        "update agent_runs set status = 'cancelled' where conversation_id = $1",
        talk,
    )
    .await;

    // A request of Marc's awaiting his decision as Marc's manager.
    let routed = format!(
        "insert into approvals (tenant_id, run_id, requested_for_user_id, tool_name, \
         tool_call_id, action_summary, payload, risk, reason, routing, approver_user_id) \
         select tenant_id, id, on_behalf_of_user_id, 'outil', 'appel', 'Action de Marc', '{{}}', \
         'write', 'Validation', 'manager', '{}' from agent_runs where conversation_id = $1",
        user::HUGO
    );
    let talk_of_marc = conversation(tenant::ID, user::MARC, "Demande de Marc", 1).await;
    let ran = "insert into agent_runs (tenant_id, conversation_id, on_behalf_of_user_id, \
               objective, budget, status) select tenant_id, id, owner_user_id, 'objectif', \
               '{}', 'completed' from conversations where id = $1";
    as_owner(ran, talk_of_marc).await;
    as_owner(&routed, talk_of_marc).await;
    refused(
        erase(&paul, user::HUGO, &hugo).await,
        409,
        "décision",
        user::HUGO,
    )
    .await;
    as_owner(
        "update approvals set status = 'expired' where requested_for_user_id = $1",
        user::MARC,
    )
    .await;

    // Standing in for Marc, absent next week.
    absence(tenant::ID, user::MARC, user::HUGO).await;
    refused(
        erase(&paul, user::HUGO, &hugo).await,
        409,
        "remplace",
        user::HUGO,
    )
    .await;
    as_owner(
        "delete from approval_delegations where delegate_user_id = $1",
        user::HUGO,
    )
    .await;

    // Leading the north team.
    let lead = format!(
        "update org_units set lead_user_id = '{}' where id = $1",
        user::HUGO
    );
    as_owner(&lead, unit::TEAM_NORTH).await;
    refused(
        erase(&paul, user::HUGO, &hugo).await,
        409,
        "dirige",
        user::HUGO,
    )
    .await;
    let back = format!(
        "update org_units set lead_user_id = '{}' where id = $1",
        user::JULIEN
    );
    as_owner(&back, unit::TEAM_NORTH).await;

    // Nothing ties him any more: erased.
    let erased = erase(&paul, user::HUGO, &hugo).await;
    assert_eq!(erased.status.as_u16(), 200, "{}", erased.text());
    assert!(!exists(user::HUGO).await);
    // And not twice.
    refused_gone(erase(&paul, user::HUGO, &hugo).await);
}

/// Answered as an account that is not there.
fn refused_gone(response: support::TestResponse) {
    assert_eq!(response.status.as_u16(), 404, "{}", response.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_another_administrator_of_its_company_erases_an_account() {
    let api = TestApi::start_with(API).await;
    let paul = api.sign_in(user::PAUL).await;
    let disabled = paul
        .patch(
            &format!("/admin/users/{}", user::LEA),
            json!({ "status": "disabled" }),
        )
        .await;
    assert_eq!(disabled.status.as_u16(), 200, "{}", disabled.text());
    let lea = email(user::LEA);

    // Not one's own account.
    let own = erase(&paul, user::PAUL, &email(user::PAUL)).await;
    assert_eq!(own.status.as_u16(), 403, "{}", own.text());
    assert!(exists(user::PAUL).await);

    // Not a manager without the permission, nor one who manages accounts.
    let marc = api.sign_in(user::MARC).await;
    refused(erase(&marc, user::LEA, &lea).await, 403, "", user::LEA).await;
    let manager = holder_of(&api, &paul, "gestion.comptes", &["user:manage"]).await;
    refused(erase(&manager, user::LEA, &lea).await, 403, "", user::LEA).await;

    // Another company's administrator finds no such account.
    let address = "admin@acme-effacement.example";
    let company = companies::create(&api, "acme-effacement", address).await;
    let accepted = accept(&api, &company.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status.as_u16(), 200, "{}", accepted.text());
    let jeanne = api.sign_in_with(address, INVITED_PASSWORD).await;
    refused_gone(erase(&jeanne, user::LEA, &lea).await);
    assert!(exists(user::LEA).await);

    // Nor anyone an account that never was.
    refused_gone(erase(&paul, Uuid::new_v4(), &lea).await);

    // Who holds the permission erases it.
    let eraser = holder_of(&api, &paul, "effacement.comptes", &["user:erase"]).await;
    let erased = erase(&eraser, user::LEA, &lea).await;
    assert_eq!(erased.status.as_u16(), 200, "{}", erased.text());
    assert!(!exists(user::LEA).await);
}
