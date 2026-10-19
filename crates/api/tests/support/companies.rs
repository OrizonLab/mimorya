//! Companies created by the operator's service (ADR-033), as `mimorya-admin
//! create-company` would, with Jeanne Martin as their first administrator.

use mimorya_api::services::provisioning::{CreatedCompany, NewCompany, create_company};

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
