//! Companies suspended and reactivated as the operator's command does
//! (`mimorya-admin`, ADR-191): the change, its refusal, the company's status
//! and its audit entries, and the demo company made active again whatever
//! becomes of a test that suspends it.

use mimorya_api::services::provisioning::{CompanyStatusChange, change_company_status};
use mimorya_core::DomainError;
use mimorya_core::demo::tenant;
use uuid::Uuid;

use super::{TestApi, owner_url};

pub async fn change(
    api: &TestApi,
    slug: &str,
    change: CompanyStatusChange,
) -> anyhow::Result<Uuid> {
    change_company_status(&api.app.services.db, slug, change).await
}

pub async fn refused(api: &TestApi, slug: &str, asked: CompanyStatusChange) -> DomainError {
    let error = change(api, slug, asked).await.expect_err("refusal");
    error.downcast::<DomainError>().expect("domain error")
}

/// The company's status, and its audit entries of `action` by `system`.
pub async fn state(api: &TestApi, tenant_id: Uuid, action: &str) -> (String, i64) {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    let status = sqlx::query_scalar("select status from tenants where id = $1")
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let entries = sqlx::query_scalar(
        "select count(*) from audit_log where action = $1 and actor_type = 'system' \
         and resource_type = 'tenant' and resource_id = $2::text and outcome = 'success'",
    )
    .bind(action)
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    (status, entries)
}

/// Makes the demo company active again when dropped, as its owner: the
/// tests after one that fails while it is suspended find it as they expect.
pub struct DemoActiveOnDrop;

impl Drop for DemoActiveOnDrop {
    fn drop(&mut self) {
        let reactivated = std::thread::spawn(|| {
            tokio::runtime::Runtime::new()
                .expect("runtime")
                .block_on(async {
                    let owner = sqlx::PgPool::connect(owner_url()).await.expect("owner");
                    sqlx::query("update tenants set status = 'active' where id = $1")
                        .bind(tenant::ID)
                        .execute(&owner)
                        .await
                        .expect("reactivated");
                });
        })
        .join();
        reactivated.expect("reactivation thread");
    }
}
