//! In-memory state of the demo connector: dataset plus plannings applied since start.

use std::sync::Mutex;

use chrono::NaiveDate;
use uuid::Uuid;

use super::dataset::{Absence, Intervention, build_dataset};
use super::dates::monday_of;

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledSlot {
    /// The intervention followed up: its readers alone see the label.
    pub intervention_id: String,
    pub technician_id: Uuid,
    pub date: NaiveDate,
    pub start_time: String,
    pub duration_hours: f64,
    pub label: String,
}

#[derive(Debug)]
pub struct DemoInterventionsStore {
    pub interventions: Vec<Intervention>,
    pub absences: Vec<Absence>,
    applied: Mutex<Vec<ScheduledSlot>>,
}

impl DemoInterventionsStore {
    /// Data of the week containing `reference`, plus the previous and next weeks.
    pub fn new(reference: NaiveDate) -> Self {
        let (interventions, absences) = build_dataset(monday_of(reference));
        Self {
            interventions,
            absences,
            applied: Mutex::new(Vec::new()),
        }
    }

    pub fn find(&self, id: &str) -> Option<&Intervention> {
        self.interventions.iter().find(|i| i.id == id)
    }

    pub fn applied(&self) -> Vec<ScheduledSlot> {
        self.applied.lock().expect("demo store").clone()
    }

    pub fn apply(&self, slots: Vec<ScheduledSlot>) {
        self.applied.lock().expect("demo store").extend(slots);
    }
}
