//! Authorization view of an intervention: every read goes through `ctx.authz`.

use mimorya_core::authz::ResourceRef;
use mimorya_core::tools::ToolContext;

use super::dataset::Intervention;

pub fn as_resource(i: &Intervention) -> ResourceRef {
    // The tenant is set by the bound authorization.
    ResourceRef::new("intervention", uuid::Uuid::nil())
        .id(i.id)
        .owner(Some(i.technician_id))
        .org_unit(Some(i.team_unit_id))
        .sensitivity(Some(i.sensitivity))
        .attribute("status", serde_json::to_value(i.status).expect("status"))
}

pub fn can_read(ctx: &ToolContext, i: &Intervention) -> bool {
    ctx.authz.check("intervention:read", as_resource(i)).allowed
}
