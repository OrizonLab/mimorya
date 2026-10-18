//! FICTIONAL demo company "Néo Maintenance" — organisation chart.

use crate::authz::{OrgUnit, OrgUnitType};

use super::ids::{unit, user};

/// Parents are listed before their children.
pub fn org_units() -> Vec<OrgUnit> {
    let unit = |id, parent_id, name: &str, unit_type, lead| OrgUnit {
        id,
        parent_id,
        name: name.into(),
        unit_type,
        lead_user_id: Some(lead),
    };
    vec![
        unit(
            unit::COMPANY,
            None,
            "Néo Maintenance",
            OrgUnitType::Company,
            user::CLAIRE,
        ),
        unit(
            unit::DIRECTION,
            Some(unit::COMPANY),
            "Direction",
            OrgUnitType::Direction,
            user::CLAIRE,
        ),
        unit(
            unit::TECHNICAL,
            Some(unit::COMPANY),
            "Service technique",
            OrgUnitType::Department,
            user::MARC,
        ),
        unit(
            unit::TEAM_NORTH,
            Some(unit::TECHNICAL),
            "Équipe Nord",
            OrgUnitType::Team,
            user::JULIEN,
        ),
        unit(
            unit::TEAM_SOUTH,
            Some(unit::TECHNICAL),
            "Équipe Sud",
            OrgUnitType::Team,
            user::NADIA,
        ),
        unit(
            unit::ADMIN,
            Some(unit::COMPANY),
            "Service administratif",
            OrgUnitType::Department,
            user::ISABELLE,
        ),
    ]
}
