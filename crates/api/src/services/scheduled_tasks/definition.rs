//! What a scheduled task asks and when, as its author writes it (ADR-080):
//! a title on one line, the instruction sent as theirs, and a weekly
//! schedule. Hidden characters are taken out: what they read is what the
//! model reads.

use mimorya_core::DomainError;
use mimorya_core::plain_text::{canonical, without_hidden};
use mimorya_core::schedule::WeeklySchedule;

pub const MAX_TITLE_CHARS: usize = 120;
pub const MAX_INSTRUCTION_CHARS: usize = 4000;

const BAD_TITLE: &str = "L’intitulé compte de 1 à 120 caractères.";
const BAD_INSTRUCTION: &str = "La demande compte de 1 à 4 000 caractères.";

/// A task as checked: what it asks, and when.
#[derive(Debug, Clone)]
pub struct CheckedTask {
    pub title: String,
    pub instruction: String,
    pub schedule: WeeklySchedule,
}

impl CheckedTask {
    pub fn check(
        title: &str,
        instruction: &str,
        weekdays: &[u8],
        time: &str,
        time_zone: &str,
    ) -> Result<Self, DomainError> {
        let title = one_line(title);
        if !(1..=MAX_TITLE_CHARS).contains(&title.chars().count()) {
            return Err(DomainError::validation(BAD_TITLE));
        }
        let instruction = canonical(without_hidden(instruction).trim());
        if !(1..=MAX_INSTRUCTION_CHARS).contains(&instruction.chars().count()) {
            return Err(DomainError::validation(BAD_INSTRUCTION));
        }
        Ok(Self {
            title,
            instruction,
            schedule: WeeklySchedule::parse(weekdays, time, time_zone)?,
        })
    }

    /// Its days as stored.
    pub fn weekdays(&self) -> Vec<i16> {
        self.schedule.days().into_iter().map(i16::from).collect()
    }
}

/// The text on one line, its blanks as single spaces, without hidden
/// characters.
fn one_line(text: &str) -> String {
    canonical(&without_hidden(text))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(title: &str, instruction: &str) -> Result<CheckedTask, DomainError> {
        CheckedTask::check(title, instruction, &[1], "08:00", "Europe/Paris")
    }

    #[test]
    fn keeps_what_is_seen_and_bounds_it() {
        let task = check("  Point\n hebdo\u{200B} ", " Fais le point.\nPuis résume. ").unwrap();
        assert_eq!(task.title, "Point hebdo");
        assert_eq!(task.instruction, "Fais le point.\nPuis résume.");
        assert_eq!(task.weekdays(), [1]);
        assert_eq!(check(" \u{200B} ", "x").unwrap_err().message, BAD_TITLE);
        assert_eq!(check(&"t".repeat(121), "x").unwrap_err().message, BAD_TITLE);
        assert!(check(&"t".repeat(120), "x").is_ok());
        assert_eq!(check("t", "  ").unwrap_err().message, BAD_INSTRUCTION);
        let long = "x".repeat(MAX_INSTRUCTION_CHARS + 1);
        assert_eq!(check("t", &long).unwrap_err().message, BAD_INSTRUCTION);
        let bad_day = CheckedTask::check("t", "x", &[9], "08:00", "UTC");
        assert!(bad_day.is_err());
    }
}
