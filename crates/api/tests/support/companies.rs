//! Companies created by the operator's service (ADR-033), as `mimorya-admin
//! create-company` would, with Jeanne Martin as their first administrator;
//! and what a company holds, table by table, as its deletion counts it
//! (ADR-192).

use mimorya_api::services::provisioning::{CreatedCompany, NewCompany, create_company};
use mimorya_db::repositories::tenant_deletion::tenant_rows;
use sqlx::PgPool;
use uuid::Uuid;

use super::{TestApi, WEB_ORIGIN};

pub fn company(slug: &str, admin_email: &str) -> NewCompany {
    NewCompany {
        name: " Acme Industrie ".into(),
        slug: slug.into(),
        admin_email: admin_email.into(),
        admin_name: "Jeanne Martin".into(),
    }
}

pub async fn create(api: &TestApi, slug: &str, admin_email: &str) -> CreatedCompany {
    create_company(&api.app.services.db, WEB_ORIGIN, company(slug, admin_email))
        .await
        .expect("company")
}

/// The rows of each table holding some of the company `tenant_id`, read as
/// the database's `owner`.
pub async fn rows_of(owner: &PgPool, tenant_id: Uuid) -> Vec<(String, i64)> {
    let mut conn = owner.acquire().await.unwrap();
    tenant_rows(&mut conn, tenant_id).await.unwrap()
}
