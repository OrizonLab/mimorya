//! A company created by the operator (ADR-033): the company, its root unit
//! (the company itself), the platform roles, and its first administrator,
//! invited by a link (no password goes through the operator). Nothing else:
//! no business role, connector, memory nor skill; the administrator builds
//! the rest from the administration. All of it or nothing.

use mimorya_core::DomainError;
use mimorya_core::accounts::{EMAIL_MAX_LEN, display_name_problem};
use mimorya_core::authz::{AssignmentScope, Sensitivity};
use mimorya_core::provisioning::{company_name_problem, slug_problem};
use mimorya_core::roles::PLATFORM_ROLE_TEMPLATES;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::org_units::create_root_unit;
use mimorya_db::repositories::role_assignments::{NewRoleAssignment, create_role_assignment};
use mimorya_db::repositories::roles::create_platform_role;
use mimorya_db::repositories::tenants::{NewTenant, create_tenant};
use mimorya_db::repositories::users::{NewUser, create_user, set_primary_unit};
use mimorya_db::{Database, is_unique_violation};
use serde_json::json;
use uuid::Uuid;
use validator::ValidateEmail;

use super::invitation::{OperatorInvitation, issue};
use super::system_entry;

/// The first administrator's roles: the base role on their own data, and
/// the workspace's administration (no business data) on the whole company.
const FIRST_ADMINISTRATOR_ROLES: [(&str, AssignmentScope); 2] = [
    ("member", AssignmentScope::Own),
    ("tenant_admin", AssignmentScope::Tenant),
];

pub struct NewCompany {
    pub name: String,
    /// Unique across the platform (`mimorya_core::provisioning`).
    pub slug: String,
    pub admin_email: String,
    pub admin_name: String,
}

impl NewCompany {
    /// Trimmed, the address lower-cased, or why it cannot be created.
    fn checked(self) -> Result<Self, DomainError> {
        let company = Self {
            name: self.name.trim().to_string(),
            slug: self.slug.trim().to_string(),
            admin_email: self.admin_email.trim().to_lowercase(),
            admin_name: self.admin_name.trim().to_string(),
        };
        let email = &company.admin_email;
        let problem = company_name_problem(&company.name)
            .or_else(|| slug_problem(&company.slug).map(String::from))
            .or_else(|| {
                (email.chars().count() as u64 > EMAIL_MAX_LEN || !email.validate_email())
                    .then(|| "L’adresse e-mail de l’administrateur n’est pas valide".into())
            })
            .or_else(|| display_name_problem(&company.admin_name, "de l’administrateur"));
        match problem {
            Some(problem) => Err(DomainError::validation(problem)),
            None => Ok(company),
        }
    }
}

pub struct CreatedCompany {
    pub tenant_id: Uuid,
    pub admin_id: Uuid,
    pub invitation: OperatorInvitation,
}

pub async fn create_company(
    db: &Database,
    web_origin: &str,
    input: NewCompany,
) -> anyhow::Result<CreatedCompany> {
    let company = input.checked()?;
    let tenant_id = Uuid::new_v4();
    let mut tx = db.begin(tenant_id).await?;
    let created = create_tenant(
        &mut tx,
        NewTenant {
            id: tenant_id,
            slug: &company.slug,
            name: &company.name,
        },
    )
    .await;
    match created {
        Ok(()) => {}
        Err(error) if is_unique_violation(&error) => {
            return Err(
                DomainError::conflict("Ce sigle est déjà celui d’une autre entreprise").into(),
            );
        }
        Err(error) => return Err(error.into()),
    }
    append_audit(
        &mut tx,
        system_entry(tenant_id, "tenant.created", "tenant", tenant_id)
            .metadata(json!({ "slug": company.slug })),
    )
    .await?;
    let root = create_root_unit(&mut tx, tenant_id, &company.name).await?;
    let mut roles = Vec::new();
    for template in PLATFORM_ROLE_TEMPLATES {
        roles.push((
            template.key,
            create_platform_role(&mut tx, tenant_id, template).await?,
        ));
    }

    let created = create_user(
        &mut tx,
        NewUser {
            tenant_id,
            email: company.admin_email,
            display_name: company.admin_name,
            job_title: None,
            clearance: Sensitivity::default(),
            password_hash: None,
        },
    )
    .await;
    let admin_id = match created {
        Ok(id) => id,
        // Addresses are unique across companies (one account, one company).
        Err(error) if is_unique_violation(&error) => {
            return Err(DomainError::conflict("Cette adresse e-mail est déjà utilisée").into());
        }
        Err(error) => return Err(error.into()),
    };
    set_primary_unit(&mut tx, tenant_id, admin_id, Some(root)).await?;
    append_audit(
        &mut tx,
        system_entry(tenant_id, "user.created", "user", admin_id)
            .metadata(json!({ "clearance": Sensitivity::default().as_str() })),
    )
    .await?;
    for (key, scope) in FIRST_ADMINISTRATOR_ROLES {
        let Some(&(_, role_id)) = roles.iter().find(|(template, _)| *template == key) else {
            anyhow::bail!("Platform role {key} is missing from the templates");
        };
        let assignment = create_role_assignment(
            &mut tx,
            NewRoleAssignment {
                tenant_id,
                user_id: admin_id,
                role_id,
                scope: scope.clone(),
                granted_by_user_id: None,
                expires_at: None,
            },
        )
        .await?;
        append_audit(
            &mut tx,
            system_entry(tenant_id, "role.assigned", "user", admin_id).metadata(json!({
                "assignmentId": assignment,
                "role": key,
                "scope": scope,
            })),
        )
        .await?;
    }
    let invitation = issue(&mut tx, admin_id, web_origin).await?;
    tx.commit().await?;
    Ok(CreatedCompany {
        tenant_id,
        admin_id,
        invitation,
    })
}
