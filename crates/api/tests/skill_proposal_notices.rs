//! The skills' administrators told by e-mail of the agent's proposals
//! (ADR-102), by the periodic pass: one e-mail to each administrator,
//! naming their skills, never the procedure nor whom it was proposed for;
//! each proposal announced once, audited, not when decided before the
//! pass; nothing without a relay nor in a process that executes no runs,
//! one process at a time; a message the relay refuses audited and not sent
//! again; an administrator who chose not to receive these not sent one,
//! audited as refused. Each company's proposals are read and told in it
//! alone. Who is told: `skill_proposal_notice_recipients.rs`.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use mimorya_api::services::skill_proposal_notice::{
    PROPOSAL_NOTICE_SWEEP, announce_skill_proposals,
};
use mimorya_core::demo::{tenant, user};
use mimorya_db::seed::DEMO_SKILL_ID;
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::json;
use support::notification_choices::{choose, chosen};
use support::skill_proposal_notices::{
    ONE, announce, announced, forget_notices, notices, one_proposal, told,
};
use support::skill_proposals::{SKILL, SKILLS, restore_the_report, waiting};
use support::{TestApi, WEB_ORIGIN, email, owner_url};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_proposal_is_announced_once_to_the_skills_administrator() {
    let api = TestApi::start_with_mailer().await;
    one_proposal(&api).await;
    assert!(api.mail.sent().iter().all(|m| m.subject != ONE));

    assert_eq!(announce(&api).await, 1);
    let sent: Vec<_> = api
        .mail
        .sent()
        .into_iter()
        .filter(|m| m.subject == ONE)
        .collect();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, email(user::PAUL));
    assert!(
        sent[0].text.contains(&format!("« {SKILL} »")),
        "{}",
        sent[0].text
    );
    assert!(sent[0].text.contains(&format!("{WEB_ORIGIN}/admin\n")));
    // Neither the procedure nor whom it was proposed for.
    assert!(!sent[0].text.contains("Relire les retards"));
    assert!(!sent[0].text.contains("Lucas"));
    assert_eq!(
        notices(tenant::ID).await,
        [(
            Some(user::PAUL),
            "success".to_string(),
            json!({ "proposals": 1, "skills": [SKILL] })
        )]
    );
    assert_eq!(announced(&api).await, [(2, true)]);

    // Once.
    assert_eq!(announce(&api).await, 0);
    assert_eq!(
        api.mail.sent().iter().filter(|m| m.subject == ONE).count(),
        1
    );
    restore_the_report(&api).await;
    forget_notices().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn proposals_are_told_together_and_a_decided_one_is_not_announced() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let run = one_proposal(&api).await;
    waiting(&api, 2, user::LUCAS, run).await;
    // Decided before the pass: nothing to tell of it.
    let rejected = paul
        .post(
            &format!("{SKILLS}/{DEMO_SKILL_ID}/rejection"),
            json!({ "version": 4 }),
        )
        .await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());

    // Another company's proposal, told to its own administrator alone.
    let other = sqlx::PgPool::connect(owner_url()).await.unwrap();
    for statement in [
        "insert into roles (id, tenant_id, key, name, description, permissions) values \
         ('00000000-0000-4000-8000-0000000f0101', $1, 'admin', 'Admin', 'Rôle', \
          array['skill:manage'])",
        "insert into role_assignments (tenant_id, user_id, role_id, scope_type) values \
         ($1, $2, '00000000-0000-4000-8000-0000000f0101', 'tenant')",
        "insert into skills (id, tenant_id, name, description) values \
         ('00000000-0000-4000-8000-0000000f0102', $1, 'procedure-ailleurs', \
          'Une procédure d’une autre entreprise')",
        "insert into skill_versions (tenant_id, skill_id, version, content, status, proposed) \
         values ($1, '00000000-0000-4000-8000-0000000f0102', 1, '1. Ailleurs.', 'candidate', \
          true)",
    ] {
        sqlx::query(statement)
            .bind(OTHER_TENANT_ID)
            .bind(OTHER_USER_ID)
            .execute(&other)
            .await
            .unwrap();
    }

    assert_eq!(announce(&api).await, 3);
    let sent = api.mail.sent();
    let to_paul: Vec<_> = sent.iter().filter(|m| m.to == email(user::PAUL)).collect();
    assert_eq!(to_paul.len(), 1, "{to_paul:?}");
    assert_eq!(
        to_paul[0].subject,
        "2 nouvelles propositions de compétences attendent votre décision"
    );
    assert!(to_paul[0].text.contains(&format!(": « {SKILL} ».")));
    assert!(!to_paul[0].text.contains("procedure-ailleurs"));
    let elsewhere: Vec<_> = sent
        .iter()
        .filter(|m| m.text.contains("procedure-ailleurs"))
        .collect();
    assert_eq!(elsewhere.len(), 1, "{elsewhere:?}");
    assert_ne!(elsewhere[0].to, email(user::PAUL));
    assert!(!elsewhere[0].text.contains(SKILL));
    assert_eq!(
        notices(tenant::ID).await,
        [(
            Some(user::PAUL),
            "success".to_string(),
            json!({ "proposals": 2, "skills": [SKILL] })
        )]
    );
    assert_eq!(notices(OTHER_TENANT_ID).await.len(), 1);
    assert_eq!(announced(&api).await, [(2, true), (3, true), (4, false)]);

    for statement in [
        "delete from skills where id = '00000000-0000-4000-8000-0000000f0102'",
        "delete from role_assignments where role_id = '00000000-0000-4000-8000-0000000f0101'",
        "delete from roles where id = '00000000-0000-4000-8000-0000000f0101'",
    ] {
        sqlx::query(statement).execute(&other).await.unwrap();
    }
    other.close().await;
    restore_the_report(&api).await;
    forget_notices().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_who_chose_not_to_receive_them_is_not_sent_one() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    choose(&paul, chosen(false, true, true)).await;
    one_proposal(&api).await;

    assert_eq!(announce(&api).await, 1);
    assert!(api.mail.sent().iter().all(|m| m.subject != ONE));
    assert_eq!(
        notices(tenant::ID).await,
        [(
            Some(user::PAUL),
            "denied".to_string(),
            json!({ "proposals": 1, "skills": [SKILL], "optedOut": true })
        )]
    );
    // Counted as told: not sent once they choose otherwise.
    choose(&paul, chosen(true, true, true)).await;
    assert_eq!(announce(&api).await, 0);
    assert!(api.mail.sent().iter().all(|m| m.subject != ONE));
    restore_the_report(&api).await;
    forget_notices().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_a_relay_nothing_is_sent_nor_recorded() {
    let api = TestApi::start().await;
    one_proposal(&api).await;
    assert_eq!(announce(&api).await, 0);
    assert_eq!(announced(&api).await, [(2, false)]);
    assert!(notices(tenant::ID).await.is_empty());
    restore_the_report(&api).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_message_the_relay_refuses_is_audited_and_not_sent_again() {
    let api = TestApi::start_with_mailer().await;
    one_proposal(&api).await;
    api.mail.fail();

    assert_eq!(announce(&api).await, 1);
    let refused = vec![(
        Some(user::PAUL),
        "failure".to_string(),
        json!({ "proposals": 1, "skills": [SKILL] }),
    )];
    assert_eq!(notices(tenant::ID).await, refused);
    assert_eq!(announced(&api).await, [(2, true)]);
    assert_eq!(announce(&api).await, 0);
    assert_eq!(notices(tenant::ID).await, refused);
    assert!(told(&api).is_empty());
    restore_the_report(&api).await;
    forget_notices().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn one_process_that_executes_runs_announces_at_a_time() {
    let api = TestApi::start_with_mailer().await;
    one_proposal(&api).await;
    // A process that executes no runs announces nothing.
    assert_eq!(
        announce_skill_proposals(&api.app.services).await.unwrap(),
        0
    );
    // Under way elsewhere: skipped, nothing announced.
    let held = api
        .app
        .services
        .db
        .try_sweep_lock(PROPOSAL_NOTICE_SWEEP)
        .await;
    let held = held.unwrap().expect("free");
    assert_eq!(announce(&api).await, 0);
    assert!(told(&api).is_empty());
    assert_eq!(announced(&api).await, [(2, false)]);
    held.release().await;

    announce(&api).await;
    // A maintenance pass may have announced it meanwhile: once it is done.
    let wait = Duration::from_secs(5);
    let settled = api.app.services.db.sweep_lock(PROPOSAL_NOTICE_SWEEP, wait);
    settled.await.unwrap().release().await;
    assert_eq!(told(&api), [email(user::PAUL)]);
    assert_eq!(announced(&api).await, [(2, true)]);
    restore_the_report(&api).await;
    forget_notices().await;
}
