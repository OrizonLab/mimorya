//! Answers shared by the routes that change something.

use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

/// A change done, with nothing else to return.
#[derive(Debug, Serialize, TS)]
pub struct Done {
    pub ok: bool,
}

impl Done {
    pub const OK: Done = Done { ok: true };
}

/// A resource created (`201`): its identifier.
#[derive(Debug, Serialize, TS)]
pub struct Created {
    pub id: Uuid,
}
