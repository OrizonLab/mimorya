//! The permission catalog: the platform's permissions and those of every
//! connector type. Roles are built from it, and `/auth/me` announces the
//! effective ones among it.

use mimorya_connectors::connector_types;
use mimorya_core::authz::{PLATFORM_PERMISSIONS, PermissionInfo};

pub fn permission_catalog() -> Vec<PermissionInfo> {
    PLATFORM_PERMISSIONS
        .iter()
        .copied()
        .chain(
            connector_types()
                .iter()
                .flat_map(|c| c.info().permissions.iter().copied()),
        )
        .collect()
}
