//! FICTIONAL demo company "Néo Maintenance" — staff and their role assignments.

use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::authz::{AssignmentScope, RoleAssignment, Sensitivity};

use super::ids::{role, unit, user};

#[derive(Debug, Clone, PartialEq)]
pub struct DemoUser {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub job_title: String,
    pub primary_unit_id: Uuid,
    pub clearance: Sensitivity,
    pub attributes: Map<String, Value>,
    pub assignments: Vec<RoleAssignment>,
}

/// Every demo user is a member (own data only) plus the given assignments.
fn demo_user(
    id: Uuid,
    login: &str,
    display_name: &str,
    job_title: &str,
    primary_unit_id: Uuid,
    assignments: Vec<RoleAssignment>,
    clearance: Sensitivity,
) -> DemoUser {
    let mut all = vec![RoleAssignment::new(role::MEMBER, AssignmentScope::Own)];
    all.extend(assignments);
    DemoUser {
        id,
        email: format!("{login}@neo-maintenance.example"),
        display_name: display_name.into(),
        job_title: job_title.into(),
        primary_unit_id,
        clearance,
        attributes: Map::new(),
        assignments: all,
    }
}

fn technician(id: Uuid, login: &str, name: &str, title: &str, team: Uuid) -> DemoUser {
    let own = RoleAssignment::new(role::TECHNICIAN, AssignmentScope::Own);
    demo_user(
        id,
        login,
        name,
        title,
        team,
        vec![own],
        Sensitivity::Internal,
    )
}

fn team_lead(id: Uuid, login: &str, name: &str, title: &str, team: Uuid) -> DemoUser {
    let assignments = vec![
        RoleAssignment::new(role::TEAM_LEAD, AssignmentScope::subtree(team)),
        RoleAssignment::new(role::TECHNICIAN, AssignmentScope::Own),
    ];
    demo_user(
        id,
        login,
        name,
        title,
        team,
        assignments,
        Sensitivity::Confidential,
    )
}

pub fn users() -> Vec<DemoUser> {
    let tenant_wide = |role_id| vec![RoleAssignment::new(role_id, AssignmentScope::Tenant)];
    let mut isabelle = demo_user(
        user::ISABELLE,
        "isabelle.laurent",
        "Isabelle Laurent",
        "Assistante administrative",
        unit::ADMIN,
        tenant_wide(role::ADMIN_STAFF),
        Sensitivity::Internal,
    );
    isabelle
        .attributes
        .insert("department".into(), json!("administratif"));

    vec![
        demo_user(
            user::CLAIRE,
            "claire.martin",
            "Claire Martin",
            "Directrice générale",
            unit::DIRECTION,
            tenant_wide(role::DIRECTOR),
            Sensitivity::Confidential,
        ),
        demo_user(
            user::MARC,
            "marc.lefevre",
            "Marc Lefèvre",
            "Responsable technique",
            unit::TECHNICAL,
            vec![RoleAssignment::new(
                role::TEAM_LEAD,
                AssignmentScope::subtree(unit::TECHNICAL),
            )],
            Sensitivity::Confidential,
        ),
        team_lead(
            user::JULIEN,
            "julien.moreau",
            "Julien Moreau",
            "Chef d’équipe Nord",
            unit::TEAM_NORTH,
        ),
        technician(
            user::LUCAS,
            "lucas.petit",
            "Lucas Petit",
            "Technicien",
            unit::TEAM_NORTH,
        ),
        technician(
            user::EMMA,
            "emma.roux",
            "Emma Roux",
            "Technicienne",
            unit::TEAM_NORTH,
        ),
        team_lead(
            user::NADIA,
            "nadia.benali",
            "Nadia Benali",
            "Cheffe d’équipe Sud",
            unit::TEAM_SOUTH,
        ),
        technician(
            user::HUGO,
            "hugo.garnier",
            "Hugo Garnier",
            "Technicien",
            unit::TEAM_SOUTH,
        ),
        technician(
            user::LEA,
            "lea.fontaine",
            "Léa Fontaine",
            "Technicienne",
            unit::TEAM_SOUTH,
        ),
        isabelle,
        demo_user(
            user::PAUL,
            "paul.renaud",
            "Paul Renaud",
            "Administrateur technique",
            unit::ADMIN,
            tenant_wide(role::TENANT_ADMIN),
            Sensitivity::Internal,
        ),
    ]
}
