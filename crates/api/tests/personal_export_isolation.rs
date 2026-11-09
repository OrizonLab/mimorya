//! Nobody's data but one's own in an export (ADR-187). Marc, Paul and the
//! administrator of another company each have something of every kind an
//! export holds, named after them; Paul's conversation is shared with
//! Marc's unit, Paul's absence has Marc stand in, and Marc decided a validation
//! of Paul's. Marc's export holds exactly his own: nothing of Paul's nor
//! Acme's, his decision on Paul's run without Paul's request, never a
//! token, a token's hash nor a sealed secret.

mod support;

use mimorya_core::demo::{tenant, unit, user};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use support::personal_belongings::{belongings, company_parts, decided_for, invited};
use support::personal_export::absence;
use support::queue::API;
use support::{TestApi, companies};

const EXPORT: &str = "/account/export";

/// `value` without its instants (`…At`, `month`), which a test cannot know.
fn timeless(value: &Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .filter(|(key, _)| !key.ends_with("At") && key.as_str() != "month")
                .map(|(key, field)| (key.clone(), timeless(field)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(timeless).collect()),
        other => other.clone(),
    }
}

/// The entries of `section` whose `field` ends with `mark`, timeless.
fn marked(export: &Value, section: &str, field: &str, mark: &str) -> Value {
    let entries = export[section].as_array().unwrap().iter();
    Value::Array(
        entries
            .filter(|entry| {
                entry[field]
                    .as_str()
                    .is_some_and(|text| text.ends_with(mark))
            })
            .map(timeless)
            .collect(),
    )
}

/// Every key of `value`, at any depth.
fn keys(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            for (key, field) in fields {
                found.push(key.to_lowercase());
                keys(field, found);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| keys(item, found)),
        _ => {}
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_export_holds_exactly_the_person_s_own_data() {
    let api = TestApi::start_with(API).await;
    let marc = api.sign_in(user::MARC).await;
    let neo = company_parts(tenant::ID, "neo", 1001).await;
    let acme = companies::create(&api, "acme-isolation", "admin@acme-isolation.example").await;
    let acme_parts = company_parts(acme.tenant_id, "acme", 2001).await;

    let his = belongings(&neo, user::MARC, "de-marc", None, 11, 10).await;
    let pauls = belongings(&neo, user::PAUL, "de-paul", Some(unit::TECHNICAL), 12, 20).await;
    belongings(&acme_parts, acme.admin_id, "d-acme", None, 21, 30).await;
    decided_for(tenant::ID, pauls.run, user::PAUL, user::MARC, "pour-paul").await;
    absence(tenant::ID, user::MARC, user::JULIEN).await;
    absence(tenant::ID, user::PAUL, user::MARC).await;
    // Paul's invitation first: a read that lost its person would find it.
    invited(
        tenant::ID,
        user::PAUL,
        Some(user::MARC),
        "invitation-de-paul",
    )
    .await;
    invited(
        tenant::ID,
        user::MARC,
        Some(user::PAUL),
        "invitation-de-marc",
    )
    .await;
    // Acme's administrator has the invitation that made her account.
    let chosen = marc
        .put(
            "/account/notifications",
            json!({ "approvals": false, "ownRuns": true, "missionEnds": false }),
        )
        .await;
    assert_eq!(chosen.status.as_u16(), 200, "{}", chosen.text());
    let exported = marc.get(EXPORT).await;
    assert_eq!(exported.status.as_u16(), 200, "{}", exported.text());
    assert_eq!(exported.headers["content-type"], "application/json");
    let export = exported.json();

    // His account, units, roles and choices, exactly.
    assert_eq!(
        timeless(&export["account"]["units"]),
        json!([{ "name": "Service technique", "primary": true }])
    );
    assert_eq!(export["account"]["leads"], json!(["Service technique"]));
    assert_eq!(
        timeless(&export["account"]["roles"]),
        json!([
            { "name": "Collaborateur", "scope": "self", "unit": null,
              "includeDescendants": true },
            { "name": "Responsable d’équipe", "scope": "org_unit",
              "unit": "Service technique", "includeDescendants": true },
        ])
    );
    assert_eq!(
        export["notifications"],
        json!({ "approvals": false, "ownRuns": true, "missionEnds": false })
    );
    // What he wrote and set up.
    assert_eq!(
        marked(&export, "conversations", "title", "de-marc"),
        json!([{
            "id": his.conversation, "title": "conversation-de-marc", "visibility": "private",
            "summary": "résumé-de-marc",
            "messaging": { "kind": "telegram", "gateway": "bot-neo", "externalId": "11",
                           "channel": "11", "thread": "1" },
            "messages": [
                { "role": "user", "content": "question-de-marc" },
                { "role": "assistant", "content": "réponse-de-marc" },
            ],
        }])
    );
    assert_eq!(
        marked(&export, "memories", "title", "de-marc"),
        json!([{ "kind": "preference", "title": "mémoire-de-marc",
                 "content": "contenu-de-marc", "status": "active" }])
    );
    assert_eq!(
        timeless(&export["scheduledTasks"]),
        json!([{ "title": "tâche-de-marc", "instruction": "consigne-de-marc",
                 "weekdays": [1], "timeOfDay": "08:30", "timeZone": "Europe/Paris" }])
    );
    assert_eq!(
        timeless(&export["triggers"]),
        json!([{ "title": "déclencheur-de-marc", "instruction": "consigne-de-marc",
                 "enabled": true }])
    );
    assert_eq!(
        timeless(&export["missions"]),
        json!([{ "title": "mission-de-marc", "description": "Une mission exportée",
                 "status": "open", "result": null }])
    );
    assert_eq!(
        timeless(&export["deliverables"]),
        json!([{ "kind": "report", "title": "livrable-de-marc",
                 "content": { "texte": "contenu-de-marc" }, "status": "draft" }])
    );
    // What the assistant did for him, and his decision on Paul's run.
    assert_eq!(
        timeless(&export["runs"]),
        json!([{
            "id": his.run, "conversationId": his.conversation, "parentRunId": null,
            "objective": "objectif-de-marc", "status": "completed", "channel": "web",
            "toolCalls": [{ "tool": "outil-de-marc", "decision": "require_approval",
                            "status": "succeeded", "input": { "note": "entrée-de-marc" } }],
        }])
    );
    assert_eq!(
        timeless(&export["approvals"]),
        json!([{ "runId": his.run, "tool": "outil-de-marc", "actionSummary": "action-de-marc",
                 "payload": { "note": "charge-de-marc" }, "status": "approved",
                 "decisionComment": "commentaire-de-marc" }])
    );
    assert_eq!(
        timeless(&export["decisions"]),
        json!([{ "tool": "outil-décidé", "status": "rejected",
                 "decisionComment": "commentaire-pour-paul" }])
    );
    assert_eq!(
        timeless(&export["usage"]),
        json!([{ "kind": "llm", "inputTokens": 10, "outputTokens": 5, "costMicros": 100 }])
    );
    // How he signs in and what he linked.
    assert_eq!(
        marked(&export, "sessions", "userAgent", "de-marc"),
        json!([{ "ip": "192.0.2.7", "userAgent": "navigateur-de-marc" }])
    );
    assert_eq!(
        timeless(&export["signIns"]),
        json!([{ "provider": "fournisseur-neo", "kind": "oidc", "subject": "sso-de-marc" }])
    );
    assert_eq!(
        timeless(&export["provisioning"]),
        json!([{ "externalId": "scim-de-marc" }])
    );
    assert_eq!(
        timeless(&export["invitation"]),
        json!({ "invitedBy": "Paul Renaud" })
    );
    assert_eq!(
        timeless(&export["absences"]),
        json!([{ "delegate": "Julien Moreau" }])
    );
    assert_eq!(
        timeless(&export["linkedAccounts"]),
        json!([{ "connector": "connecteur-neo", "subject": "sujet-de-marc",
                 "address": "de-marc@exemple.test", "scopes": "Mail.Read" }])
    );
    assert_eq!(
        timeless(&export["messagingIdentities"]),
        json!([{ "kind": "telegram", "gateway": "bot-neo", "externalId": "11",
                 "displayName": "nom-de-marc", "address": null }])
    );
    assert_eq!(
        timeless(&export["serverAuthorizations"]),
        json!([{ "connector": "connecteur-neo", "scopes": "portée-de-marc" }])
    );

    // Nothing of Paul's nor Acme's; never Paul's request.
    let text = exported.text();
    for theirs in ["de-paul", "d-acme", "action-pour-paul", "charge-pour-paul"] {
        assert!(!text.contains(theirs), "{theirs} exported");
    }
    // Never a token, its hash, a sealed secret nor a password hash.
    let hook_hash = format!("{:x}", Sha256::digest("de-marc"));
    let invitation_hash = format!("{:x}", Sha256::digest("invitation-de-marc"));
    for secret in [
        "hash-de-marc",
        &hook_hash,
        &invitation_hash,
        "chiffré",
        "accès-",
        "$argon2",
    ] {
        assert!(!text.contains(secret), "{secret} exported");
    }
    let mut found = Vec::new();
    keys(&export, &mut found);
    for key in found {
        let held = ["cipher", "hash", "secret", "password", "authtag", "token"]
            .iter()
            .any(|word| key.contains(word))
            || key == "iv";
        assert!(
            !held || key == "inputtokens" || key == "outputtokens",
            "{key} exported"
        );
    }
}
