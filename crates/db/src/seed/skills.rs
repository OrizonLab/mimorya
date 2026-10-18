//! Demo skill: the weekly interventions report, active and approved (v1).

use mimorya_core::demo::{demo_id, tenant, user};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

pub const DEMO_SKILL_ID: Uuid = demo_id(600);

const SKILL_NAME: &str = "synthese-hebdo-interventions";

const WEEKLY_REPORT_SKILL: &str = r#"---
name: synthese-hebdo-interventions
description: Produire la synthèse hebdomadaire des interventions d'un périmètre et proposer un planning de suivi des interventions non terminées.
license: Proprietary
metadata:
  mimorya:
    category: operations
    tools: [interventions_search, interventions_get, team_availability, planning_propose]
---

# Synthèse hebdomadaire des interventions

1. Rechercher les interventions de la période demandée (par défaut : semaine en cours).
2. Pour chaque intervention non terminée ou signalée en anomalie, consulter le détail.
3. Classer : non terminées, terminées avec anomalie, terminées.
4. Pour chaque intervention non terminée, identifier la cause (pièce, accès client,
   compte rendu incomplet) et l'action de suivi.
5. Consulter les disponibilités de l'équipe et proposer un créneau par suivi
   (sans double réservation, en respectant les procédures d'équipe connues).
6. Présenter : tableau de synthèse (identifiants INT-xxxx), anomalies, planning proposé,
   actions nécessitant validation.
"#;

pub(super) async fn insert(conn: &mut PgConnection) -> sqlx::Result<()> {
    sqlx::query(
        "insert into skills (id, tenant_id, name, description, status, created_by_user_id) \
         values ($1, $2, $3, $4, 'active', $5)",
    )
    .bind(DEMO_SKILL_ID)
    .bind(tenant::ID)
    .bind(SKILL_NAME)
    .bind("Produire la synthèse hebdomadaire des interventions d’un périmètre et proposer un planning de suivi.")
    .bind(user::MARC)
    .execute(&mut *conn)
    .await?;
    let version_id: Uuid = sqlx::query_scalar(
        "insert into skill_versions (tenant_id, skill_id, version, content, frontmatter, changelog, status, \
         created_by_user_id, approved_by_user_id, approved_at) \
         values ($1, $2, 1, $3, $4, 'Version initiale (démonstration)', 'active', $5, $6, now()) returning id",
    )
    .bind(tenant::ID)
    .bind(DEMO_SKILL_ID)
    .bind(WEEKLY_REPORT_SKILL)
    .bind(json!({ "name": SKILL_NAME }))
    .bind(user::MARC)
    .bind(user::PAUL)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("update skills set active_version_id = $2 where id = $1")
        .bind(DEMO_SKILL_ID)
        .bind(version_id)
        .execute(conn)
        .await?;
    Ok(())
}
