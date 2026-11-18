//! Companies created by the operator's service (ADR-033), as `mimorya-admin
//! create-company` would, with Jeanne Martin as their first administrator;
//! the platform's model supplied to one, as `set-model-supply` would
//! (ADR-208); and what a company holds, table by table, as its deletion
//! counts it (ADR-192).

use mimorya_api::services::provisioning::{
    CapChoice, CreatedCompany, NewCompany, SupplyChange, change_model_supply, create_company,
};
use mimorya_core::model_supply::SupplyFormula;
use mimorya_db::repositories::tenant_deletion::tenant_rows;
use sqlx::PgPool;
use uuid::Uuid;

use super::{TestApi, WEB_ORIGIN, owner_url};

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

/// The operator supplies the company `slug` the platform's model, capped at
/// the default: a company created has none of its own (ADR-208).
pub async fn supplied(slug: &str) {
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let capped = SupplyChange::Formula {
        formula: SupplyFormula::Capped,
        cap: CapChoice::Default,
    };
    change_model_supply(&owner, slug, capped).await.unwrap();
    owner.close().await;
}
