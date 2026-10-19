//! Working hours, busy slots and time helpers for the demo planning.

use chrono::NaiveDate;
use uuid::Uuid;

use super::dataset::{Intervention, InterventionStatus};
use super::store::DemoInterventionsStore;

/// Working day in minutes since midnight (08:00 – 17:00).
pub const WORK_START: u32 = 8 * 60;
pub const WORK_END: u32 = 17 * 60;

pub fn to_minutes(time: &str) -> u32 {
    let (hours, minutes) = time.split_once(':').unwrap_or((time, "0"));
    hours.parse::<u32>().unwrap_or(0) * 60 + minutes.parse::<u32>().unwrap_or(0)
}

pub fn to_time(minutes: u32) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// End of a slot, in minutes since midnight.
pub fn end_of(start: u32, duration_hours: f64) -> u32 {
    start + (duration_hours * 60.0).round() as u32
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Busy {
    pub start: u32,
    pub end: u32,
    pub label: String,
}

impl Busy {
    pub fn overlaps(&self, start: u32, end: u32) -> bool {
        start < self.end && end > self.start
    }
}

/// How a slot taken by an intervention the user may not read is shown: taken,
/// without saying by what (ADR-022: a deny on `intervention:read` holds here too).
pub const HIDDEN_SLOT: &str = "Occupé (intervention hors de votre périmètre)";

/// The technician's busy slots that day; `readable` tells which interventions
/// the user may read, the others are only shown as taken.
pub fn busy_slots(
    store: &DemoInterventionsStore,
    technician_id: Uuid,
    date: NaiveDate,
    readable: &dyn Fn(&Intervention) -> bool,
) -> Vec<Busy> {
    let mut busy: Vec<Busy> = store
        .interventions
        .iter()
        .filter(|i| {
            i.technician_id == technician_id
                && i.date == date
                && i.status != InterventionStatus::Cancelled
        })
        .map(|i| {
            let start = to_minutes(i.start_time);
            Busy {
                start,
                end: end_of(start, i.duration_hours),
                label: if readable(i) {
                    format!("{} {}", i.id, i.kind)
                } else {
                    HIDDEN_SLOT.to_string()
                },
            }
        })
        .collect();
    busy.extend(
        store
            .absences
            .iter()
            .filter(|a| a.technician_id == technician_id && a.date == date)
            .map(|a| Busy {
                start: to_minutes(a.start_time),
                end: to_minutes(a.end_time),
                label: a.reason.to_string(),
            }),
    );
    busy.extend(
        store
            .applied()
            .into_iter()
            .filter(|s| s.technician_id == technician_id && s.date == date)
            .map(|s| {
                let start = to_minutes(&s.start_time);
                let shown = store
                    .interventions
                    .iter()
                    .find(|i| i.id == s.intervention_id)
                    .is_some_and(readable);
                Busy {
                    start,
                    end: end_of(start, s.duration_hours),
                    label: if shown { s.label } else { HIDDEN_SLOT.into() },
                }
            }),
    );
    busy.sort_by_key(|b| b.start);
    busy
}
