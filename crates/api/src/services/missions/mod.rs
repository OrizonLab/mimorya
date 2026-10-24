//! Missions (ADR-089): work a person entrusts to their agent, carried out in
//! the background in stages, each a run of the mission's conversation.
//! `definition` checks what is entrusted and holds a mission's limits,
//! `entrust` opens one and lists them, `stages` opens each stage and
//! carries the mission on, or ends it, as one ends, `stop` stops one at its
//! owner's request.

mod definition;
mod entrust;
mod stages;
mod stop;

pub use definition::{CheckedMission, MISSION_LIMITS, MissionLimits};
pub use entrust::{MAX_UNDER_WAY, MissionInput, entrust_mission, mission, missions};
pub use stages::after_stage;
pub use stop::{STOPPED, stop_mission};
