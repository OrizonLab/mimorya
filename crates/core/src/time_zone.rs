//! The platform's time zone: the model is told today's date in it, and the
//! days a tool is given are read in it (ADR-117), so that both agree.

use chrono_tz::Tz;

pub const PLATFORM_TIME_ZONE: Tz = chrono_tz::Europe::Paris;
