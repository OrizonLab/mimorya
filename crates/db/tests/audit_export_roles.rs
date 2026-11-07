//! What migration 0079 grants the platform's roles of the companies already
//! there (ADR-181): the platform's administrator role exports the audit log;
//! a company's own roles, even keyed alike, and the other platform roles are
//! left alone, and a second run changes nothing.

mod support;

use uuid::Uuid;

/// The part of the migration that updates the roles already there.
fn role_updates() -> &'static str {
    let migration = include_str!("../migrations/0079_audit_export_tokens.sql");
    let start = migration
        .find("-- The platform's administrator role of the companies already there")
        .expect("the roles' part of the migration");
    &migration[start..]
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_platform_s_administrators_already_there_export_the_audit_log() {
    let urls = support::urls(|_| {});
    let db = support::connect(&urls.owner).await;
    let (platform, own) = (Uuid::new_v4(), Uuid::new_v4());
    for tenant in [platform, own] {
        sqlx::query("insert into tenants (id, slug, name) values ($1, $2, 'Avant l’export')")
            .bind(tenant)
            .bind(format!("avant-export-{tenant}"))
            .execute(db.pool())
            .await
            .unwrap();
    }
    for (tenant, key, system, permissions) in [
        (
            platform,
            "tenant_admin",
            true,
            vec!["audit:read", "user:manage"],
        ),
        (platform, "member", true, vec!["agent:use"]),
        (platform, "auditor", false, vec!["audit:read"]),
        // A company's own role that carries the platform role's key.
        (own, "tenant_admin", false, vec!["audit:read"]),
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

    let roles: Vec<(bool, String, Vec<String>)> = sqlx::query_as(
        "select tenant_id = $1, key, permissions from roles \
         where tenant_id in ($1, $2) order by tenant_id = $1 desc, key",
    )
    .bind(platform)
    .bind(own)
    .fetch_all(db.pool())
    .await
    .unwrap();
    let held = |permissions: &[&str]| -> Vec<String> {
        permissions.iter().map(|p| p.to_string()).collect()
    };
    assert_eq!(
        roles,
        [
            (true, "auditor".to_string(), held(&["audit:read"])),
            (true, "member".to_string(), held(&["agent:use"])),
            (
                true,
                "tenant_admin".to_string(),
                held(&["audit:read", "user:manage", "audit:export"])
            ),
            (false, "tenant_admin".to_string(), held(&["audit:read"])),
        ]
    );
    sqlx::query("delete from tenants where id in ($1, $2)")
        .bind(platform)
        .bind(own)
        .execute(db.pool())
        .await
        .unwrap();
}
