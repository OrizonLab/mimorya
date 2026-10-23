//! Requests a person asks their agent to make on a schedule (ADR-080):
//! what and when, as they write it (`definition`), their own tasks
//! (`tasks`), and the occurrences fired when due (`firing`).

mod definition;
mod firing;
mod tasks;

pub use definition::{CheckedTask, MAX_INSTRUCTION_CHARS, MAX_TITLE_CHARS};
pub use firing::{CATCH_UP, Firing, fire_due_tasks, fire_task};
pub use tasks::{
    MAX_TASKS, TaskChanges, TaskInput, change_task, create_task, delete_task, scheduled_tasks,
};
