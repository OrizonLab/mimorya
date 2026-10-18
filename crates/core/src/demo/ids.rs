//! FICTIONAL demo company used by the seed, tests and the reference scenario
//! ("PME de services techniques"). All names and data are invented.
//! Identifiers are deterministic so that tests and seeds stay reproducible.

use uuid::Uuid;

/// `00000000-0000-4000-8000-<n on 12 digits>`.
pub const fn demo_id(n: u64) -> Uuid {
    let mut digits: u128 = 0;
    let mut shift = 0;
    let mut rest = n;
    while rest > 0 {
        digits |= ((rest % 10) as u128) << shift;
        shift += 4;
        rest /= 10;
    }
    Uuid::from_u128((0x4000u128 << 64) | (0x8000u128 << 48) | digits)
}

pub mod tenant {
    use uuid::Uuid;

    pub const ID: Uuid = super::demo_id(1);
    pub const SLUG: &str = "neo-maintenance";
    pub const NAME: &str = "Néo Maintenance (démo)";
}

pub mod unit {
    use uuid::Uuid;

    use super::demo_id;

    pub const COMPANY: Uuid = demo_id(100);
    pub const DIRECTION: Uuid = demo_id(101);
    pub const TECHNICAL: Uuid = demo_id(102);
    pub const TEAM_NORTH: Uuid = demo_id(103);
    pub const TEAM_SOUTH: Uuid = demo_id(104);
    pub const ADMIN: Uuid = demo_id(105);
}

pub mod user {
    use uuid::Uuid;

    use super::demo_id;

    pub const CLAIRE: Uuid = demo_id(200);
    pub const MARC: Uuid = demo_id(201);
    pub const JULIEN: Uuid = demo_id(202);
    pub const LUCAS: Uuid = demo_id(203);
    pub const EMMA: Uuid = demo_id(204);
    pub const NADIA: Uuid = demo_id(205);
    pub const HUGO: Uuid = demo_id(206);
    pub const LEA: Uuid = demo_id(207);
    pub const ISABELLE: Uuid = demo_id(208);
    pub const PAUL: Uuid = demo_id(209);
}

pub mod role {
    use uuid::Uuid;

    use super::demo_id;

    pub const MEMBER: Uuid = demo_id(300);
    pub const TENANT_ADMIN: Uuid = demo_id(301);
    pub const KNOWLEDGE_MANAGER: Uuid = demo_id(302);
    pub const TECHNICIAN: Uuid = demo_id(310);
    pub const TEAM_LEAD: Uuid = demo_id(311);
    pub const DIRECTOR: Uuid = demo_id(312);
    pub const ADMIN_STAFF: Uuid = demo_id(313);
}
