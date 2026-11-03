//! Allow policies take two administrators (ADR-168): one proposes, another
//! approves, and only then does the policy open anything; a change applies
//! only while its policy stands as proposed; an allow policy is switched off
//! or deleted directly. Refusals: `policy_proposal_refusals.rs`.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::policy_proposals::{
    approve, approved_policy, audited, completed_interventions, completed_only, listed, propose,
    reads_south, second_admin,
};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_allow_policy_opens_nothing_until_another_administrator_approves_it() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (claire, claire_id) = second_admin(&api, &paul, "claire.politiques").await;
    assert!(!reads_south(&api, user::LUCAS, "completed").await);

    let id = propose(&paul, completed_interventions()).await;
    // Proposed, nothing is open yet, and nothing is written as a policy.
    assert!(!reads_south(&api, user::LUCAS, "completed").await);
    let policies = paul.get("/admin/policies").await.json();
    assert!(
        policies["policies"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["effect"] != "allow"),
        "{policies}"
    );
    let seen_by_paul = listed(&paul, &id).await;
    assert_eq!(seen_by_paul["status"], "pending");
    assert_eq!(seen_by_paul["mine"], true);
    assert_eq!(seen_by_paul["proposedBy"]["id"], json!(user::PAUL));
    assert_eq!(seen_by_paul["policyId"], json!(null));
    assert_eq!(listed(&claire, &id).await["mine"], false);

    // Its author never approves it.
    assert_eq!(approve(&paul, &id).await.status, StatusCode::FORBIDDEN);
    assert!(!reads_south(&api, user::LUCAS, "completed").await);

    let approved = approve(&claire, &id).await;
    assert_eq!(approved.status, StatusCode::OK, "{}", approved.text());
    // Open at once: only the completed interventions, outside the role's
    // own data, to whoever holds the permission somewhere, never to others.
    assert!(reads_south(&api, user::LUCAS, "completed").await);
    assert!(!reads_south(&api, user::LUCAS, "planned").await);
    assert!(!reads_south(&api, user::PAUL, "completed").await);

    let decided = listed(&paul, &id).await;
    assert_eq!(decided["status"], "approved");
    assert_eq!(decided["decidedBy"]["id"], json!(claire_id));
    assert_eq!(approve(&claire, &id).await.status, StatusCode::CONFLICT);

    let policy_id = approved_policy(&api, &id).await;
    let listing = paul.get("/admin/policies").await.json();
    let policy = listing["policies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == json!(policy_id))
        .expect("the approved policy is listed")
        .clone();
    assert_eq!(policy["effect"], "allow");
    assert_eq!(policy["editable"], false);
    assert_eq!(policy["condition"], completed_only());

    // Each step audited by whoever took it.
    let created = audited(&api, "policy_proposal.created", &id).await;
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].0, user::PAUL);
    assert_eq!(created[0].1["permissions"], json!(["intervention:read"]));
    let written = audited(&api, "policy.created", &policy_id).await;
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].0, claire_id);
    assert_eq!(written[0].1["proposalId"], json!(id));
    assert_eq!(written[0].1["proposedByUserId"], json!(user::PAUL));
    assert_eq!(written[0].1["effect"], "allow");

    // Deleted directly: it only took access away.
    let url = format!("/admin/policies/{policy_id}");
    assert_eq!(paul.delete(&url).await.status, StatusCode::OK);
    assert!(!reads_south(&api, user::LUCAS, "completed").await);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_change_applies_only_while_its_policy_stands_as_proposed() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (claire, _) = second_admin(&api, &paul, "claire.changements").await;
    let first = propose(&paul, completed_interventions()).await;
    assert_eq!(approve(&claire, &first).await.status, StatusCode::OK);
    let policy_id = approved_policy(&api, &first).await;
    let url = format!("/admin/policies/{policy_id}");

    // Changed only through a proposal; switched off directly.
    let renamed = paul.patch(&url, json!({ "description": "Renommée" })).await;
    assert_eq!(renamed.status, StatusCode::FORBIDDEN, "{}", renamed.text());
    let unchanged = paul
        .post(
            "/admin/policy-proposals",
            json!({ "policyId": policy_id, "description": completed_interventions()["description"],
                    "permissions": ["intervention:read"], "condition": completed_only() }),
        )
        .await;
    assert_eq!(
        unchanged.status,
        StatusCode::BAD_REQUEST,
        "{}",
        unchanged.text()
    );

    // Two changes of one policy: the first approved outdates the other.
    let cancelled = json!({ "op": "eq", "left": { "attr": "resource.attributes.status" }, "right": { "value": "cancelled" } });
    let to_cancelled = propose(
        &paul,
        json!({ "policyId": policy_id, "description": "Les interventions annulées aussi",
                "permissions": ["intervention:read"],
                "condition": { "any": [completed_only(), cancelled.clone()] } }),
    )
    .await;
    let switched_off = propose(
        &claire,
        json!({ "policyId": policy_id, "description": "En pause",
                "permissions": ["intervention:read"], "condition": completed_only(),
                "enabled": false }),
    )
    .await;
    assert_eq!(
        listed(&paul, &to_cancelled).await["policyId"],
        json!(policy_id)
    );
    let widened = approve(&claire, &to_cancelled).await;
    assert_eq!(widened.status, StatusCode::OK, "{}", widened.text());
    assert!(reads_south(&api, user::LUCAS, "cancelled").await);
    assert_eq!(listed(&paul, &switched_off).await["outdated"], true);
    let stale = approve(&paul, &switched_off).await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{}", stale.text());
    assert!(reads_south(&api, user::LUCAS, "cancelled").await);

    let updated = audited(&api, "policy.updated", &policy_id).await;
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0].1["proposalId"], json!(to_cancelled));
    assert_eq!(updated[0].1["previousCondition"], completed_only());

    // A switch-off applies at once, and outdates what was proposed before it.
    let pending = propose(
        &paul,
        json!({ "policyId": policy_id, "description": "Les interventions terminées seules",
                "permissions": ["intervention:read"], "condition": completed_only() }),
    )
    .await;
    let off = paul.patch(&url, json!({ "enabled": false })).await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text());
    assert!(!reads_south(&api, user::LUCAS, "completed").await);
    assert_eq!(listed(&paul, &pending).await["outdated"], true);
    assert_eq!(
        approve(&claire, &pending).await.status,
        StatusCode::CONFLICT
    );

    // Enabled again only through a proposal another administrator approves.
    let again = paul.patch(&url, json!({ "enabled": true })).await;
    assert_eq!(again.status, StatusCode::FORBIDDEN);
    let enable = propose(
        &paul,
        json!({ "policyId": policy_id, "description": "Les interventions terminées seules",
                "permissions": ["intervention:read"], "condition": completed_only() }),
    )
    .await;
    assert_eq!(approve(&claire, &enable).await.status, StatusCode::OK);
    assert!(reads_south(&api, user::LUCAS, "completed").await);
    assert!(!reads_south(&api, user::LUCAS, "cancelled").await);

    // Deleted, it takes its proposals with it.
    let last = propose(
        &claire,
        json!({ "policyId": policy_id, "description": "Une dernière",
                "permissions": ["intervention:read"], "condition": cancelled }),
    )
    .await;
    assert_eq!(paul.delete(&url).await.status, StatusCode::OK);
    assert_eq!(listed(&paul, &last).await, json!(null));
    let deleted = audited(&api, "policy.deleted", &policy_id).await;
    assert_eq!(deleted[0].1["effect"], "allow");
    assert!(!reads_south(&api, user::LUCAS, "completed").await);
}
