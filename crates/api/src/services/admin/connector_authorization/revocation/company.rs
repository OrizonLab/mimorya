//! Every OAuth authorization of a company taken at once, the company's and
//! each person's, connector by connector, under the declaration each holds
//! then (ADR-172, ADR-174): what the operator's release of a suspended
//! company revokes before it is deleted (ADR-193).

use mimorya_db::TenantTx;
use mimorya_db::repositories::connector_authorizations::take_connector_authorization;
use mimorya_db::repositories::connector_person_authorizations::take_person_authorizations_of_connector;
use mimorya_db::repositories::connectors::tenant_connector_ids;
use uuid::Uuid;

use super::super::super::connectors::ConnectorAdmin;
use super::persons::declared;
use super::{Revocation, taken};

/// Takes, in `tx`, every authorization the company `tenant_id` holds of its
/// connectors: one revocation per connector that had one.
pub async fn take_company_authorizations(
    tx: &mut TenantTx,
    admin: &ConnectorAdmin<'_>,
    tenant_id: Uuid,
) -> anyhow::Result<Vec<Revocation>> {
    let mut revocations = Vec::new();
    for connector_id in tenant_connector_ids(tx, tenant_id).await? {
        let company = take_connector_authorization(tx, connector_id).await?;
        let persons = take_person_authorizations_of_connector(tx, connector_id).await?;
        let declared = declared(tx, connector_id).await?;
        let declared = declared.as_ref();
        if let Some(revocation) = taken(
            tx,
            admin,
            tenant_id,
            connector_id,
            declared,
            company,
            persons,
        )
        .await?
        {
            revocations.push(revocation);
        }
    }
    Ok(revocations)
}
