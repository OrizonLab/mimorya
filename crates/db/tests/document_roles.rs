//! What migration 0039 grants the platform's roles of the companies already
//! there (ADR-095): every member reads the documents, a knowledge manager
//! deposits them too, their descriptions say so; a company's own roles are
//! left alone, and a second run changes nothing.

mod support;

use mimorya_core::roles::role_template;
use uuid::Uuid;

/// The part of the migration that updates the roles already there.
fn role_updates() -> &'static str {
    let migration = include_str!("../migrations/0039_documents.sql");
    let start = migration
        .find("-- The platform's roles of the companies already there")
        .expect("the roles' part of the migration");
    &migration[start..]
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_platform_s_roles_already_there_gain_the_documents() {
    let urls = support::urls(|_| {});
    let db = support::connect(&urls.owner).await;
    let tenant = Uuid::new_v4();
    sqlx::query("insert into tenants (id, slug, name) values ($1, $2, 'Avant les documents')")
        .bind(tenant)
        .bind(format!("avant-documents-{tenant}"))
        .execute(db.pool())
        .await
        .unwrap();
    let before = [
        (
            "member",
            true,
            vec!["agent:use", "memory:read"],
            "Accès de base : converser avec l’agent, consulter l’organisation et la mémoire.",
        ),
        (
            "knowledge_manager",
            true,
            vec!["memory:read", "memory:write", "skill:read"],
            "Alimente et valide la mémoire partagée de son périmètre.",
        ),
        (
            "technician",
            false,
            vec!["memory:read"],
            "Rôle de l’entreprise.",
        ),
    ];
    for (key, system, permissions, description) in &before {
        sqlx::query(
            "insert into roles (tenant_id, key, name, description, permissions, is_system) \
             values ($1, $2, $2, $3, $4, $5)",
        )
        .bind(tenant)
        .bind(key)
        .bind(description)
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

    let roles: Vec<(String, Vec<String>, String)> = sqlx::query_as(
        "select key, permissions, description from roles where tenant_id = $1 order by key",
    )
    .bind(tenant)
    .fetch_all(db.pool())
    .await
    .unwrap();
    let template = |key| role_template(key).expect("a template");
    assert_eq!(
        roles,
        [
            (
                "knowledge_manager".to_string(),
                vec![
                    "memory:read".to_string(),
                    "memory:write".into(),
                    "skill:read".into(),
                    "document:read".into(),
                    "document:write".into(),
                ],
                template("knowledge_manager").description.to_string(),
            ),
            (
                "member".to_string(),
                vec![
                    "agent:use".to_string(),
                    "memory:read".into(),
                    "document:read".into()
                ],
                template("member").description.to_string(),
            ),
            (
                "technician".to_string(),
                vec!["memory:read".to_string()],
                "Rôle de l’entreprise.".to_string(),
            ),
        ]
    );
    sqlx::query("delete from tenants where id = $1")
        .bind(tenant)
        .execute(db.pool())
        .await
        .unwrap();
}
