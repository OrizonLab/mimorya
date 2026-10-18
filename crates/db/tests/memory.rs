//! Memory visibility applied in SQL.

mod support;

use mimorya_core::authz::{Actor, Sensitivity};
use mimorya_core::demo::{self, tenant, unit, user};
use mimorya_core::memory::{MemoryKind, MemoryScope, MemoryStatus, memory_read_filter};
use mimorya_db::Database;
use mimorya_db::repositories::memory::{
    MemorySource, NewMemory, create_memory, list_visible_memory, search_memory, to_or_tsquery,
};
use uuid::Uuid;

/// Adds the other team's procedure once, right after the reset.
fn add_team_south_procedure(urls: &mimorya_db::testing::TestDatabaseUrls) {
    let app = urls.app.clone();
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async move {
            let db = support::connect(&app).await;
            let mut tx = db.begin(tenant::ID).await.unwrap();
            create_memory(
                &mut tx,
                NewMemory {
                    tenant_id: tenant::ID,
                    scope: MemoryScope::OrgUnit,
                    owner_user_id: None,
                    org_unit_id: Some(unit::TEAM_SOUTH),
                    kind: MemoryKind::Procedure,
                    title: "Retours chantier équipe Sud".into(),
                    content: "Les retours chantier de l’équipe Sud ont lieu le mardi matin.".into(),
                    sensitivity: Sensitivity::Internal,
                    status: MemoryStatus::Active,
                    source: MemorySource::manual(None),
                    created_by_user_id: None,
                    created_by_run_id: None,
                },
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        });
}

async fn app() -> Database {
    support::connect(&support::urls(add_team_south_procedure).app).await
}

async fn visible_titles(user_id: Uuid) -> Vec<String> {
    let db = app().await;
    let engine = demo::authorization_engine();
    let subject = demo::subject(user_id);
    let filter = memory_read_filter(&engine, &subject, &Actor::user());
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let rows = list_visible_memory(&mut tx, &filter, subject.clearance(), false, 200)
        .await
        .unwrap();
    let mut titles: Vec<String> = rows.into_iter().map(|r| r.title).collect();
    titles.sort();
    titles
}

#[test]
fn builds_or_full_text_queries_from_natural_language() {
    assert_eq!(
        to_or_tsquery("Retours chantier : équipe Nord ?").as_deref(),
        Some("retours | chantier | équipe | nord")
    );
    assert_eq!(to_or_tsquery("a ' ; --"), None);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_technician_sees_personal_team_department_and_company_memory_only() {
    assert_eq!(
        visible_titles(user::LUCAS).await,
        vec![
            "Clôture d’une intervention",
            "Horaires d’intervention",
            "Prévenance des déplacements",
            "Retours chantier équipe Nord",
        ]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_manager_never_sees_a_collaborators_personal_memory() {
    let titles = visible_titles(user::MARC).await;
    assert!(titles.contains(&"Format des synthèses".to_string()));
    assert!(titles.contains(&"Retours chantier équipe Sud".to_string()));
    assert!(!titles.contains(&"Prévenance des déplacements".to_string()));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn ranks_full_text_matches_within_the_visible_set_only() {
    let db = app().await;
    let engine = demo::authorization_engine();
    let subject = demo::subject(user::LUCAS);
    let filter = memory_read_filter(&engine, &subject, &Actor::user());
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let hits = search_memory(
        &mut tx,
        &filter,
        "Quand ont lieu les retours chantier ?",
        5,
        Sensitivity::Internal,
    )
    .await
    .unwrap();
    let titles: Vec<&str> = hits.iter().map(|h| h.title.as_str()).collect();
    assert_eq!(titles.first(), Some(&"Retours chantier équipe Nord"));
    assert!(!titles.contains(&"Retours chantier équipe Sud"));
}
