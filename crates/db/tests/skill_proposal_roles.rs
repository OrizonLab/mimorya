//! What migration 0042 grants the platform's roles of the companies already
//! there (ADR-100): every member proposes skills, a knowledge manager too;
//! a company's own roles are left alone, and a second run changes nothing.

mod support;

use uuid::Uuid;

/// The part of the migration that updates the roles already there.
fn role_updates() -> &'static str {
    let migration = include_str!("../migrations/0042_skill_proposals.sql");
    let start = migration
        .find("-- The platform's roles of the companies already there")
        .expect("the roles' part of the migration");
    &migration[start..]
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_platform_s_roles_already_there_may_propose_skills() {
    let urls = support::urls(|_| {});
    let db = support::connect(&urls.owner).await;
    let tenant = Uuid::new_v4();
    sqlx::query("insert into tenants (id, slug, name) values ($1, $2, 'Avant les propositions')")
        .bind(tenant)
        .bind(format!("avant-propositions-{tenant}"))
        .execute(db.pool())
        .await
        .unwrap();
    for (key, system, permissions) in [
        ("member", true, vec!["agent:use", "skill:read"]),
        (
            "knowledge_manager",
            true,
            vec!["memory:write", "skill:read"],
        ),
        ("tenant_admin", true, vec!["skill:manage"]),
        ("technician", false, vec!["skill:read"]),
    ] {
        sqlx::query(
            "insert into roles (tenant_id, key, name, description, permissions, is_system) \
             values ($1, $2, $2, 'Rôle', $3, $4)",
        )
        .bind(tenant)
        .bind(key)
        .bind(permissions)
        .bind(system)
        .execute(db.pool())
        .await
        .unwrap();
    }

    for _ in 0..2 {
        sqlx::raw_sql(role_updates())
            .execute(db.pool())
            .await
            .unwrap();
    }

    let roles: Vec<(String, Vec<String>)> =
        sqlx::query_as("select key, permissions from roles where tenant_id = $1 order by key")
            .bind(tenant)
            .fetch_all(db.pool())
            .await
            .unwrap();
    let held = |permissions: &[&str]| -> Vec<String> {
        permissions.iter().map(|p| p.to_string()).collect()
    };
    assert_eq!(
        roles,
        [
            (
                "knowledge_manager".to_string(),
                held(&["memory:write", "skill:read", "skill:propose"])
            ),
            (
                "member".to_string(),
                held(&["agent:use", "skill:read", "skill:propose"])
            ),
            ("technician".to_string(), held(&["skill:read"])),
            ("tenant_admin".to_string(), held(&["skill:manage"])),
        ]
    );
    sqlx::query("delete from tenants where id = $1")
        .bind(tenant)
        .execute(db.pool())
        .await
        .unwrap();
}
