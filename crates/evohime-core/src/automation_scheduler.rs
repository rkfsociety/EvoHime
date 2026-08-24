//! Deterministic Core-owned scheduler decisions for automation (plan 18.1).
//!
//! The scheduler produces a bounded slot decision. It does not execute an
//! activity and it does not own approval or provider state. A caller persists
//! the returned slot/idempotency key before handing the trigger to the normal
//! automation runtime.

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, TimeZone, Utc};

pub const MAX_MISSED_SLOTS: u32 = 8;
pub const DEFAULT_MISSED_GRACE_MS: i64 = 5 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailySchedule {
    pub hour: u8,
    pub minute: u8,
    pub timezone: FixedOffset,
    pub missed_grace_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerCursor {
    pub last_slot: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulerDecision {
    NotDue,
    Trigger {
        slot: String,
        idempotency_key: String,
    },
    Missed {
        slot: String,
        idempotency_key: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerError {
    InvalidHour,
    InvalidMinute,
    InvalidGrace,
    InvalidTimezone,
}

impl DailySchedule {
    pub fn new(
        hour: u8,
        minute: u8,
        timezone_minutes: i32,
        missed_grace_ms: i64,
    ) -> Result<Self, SchedulerError> {
        if hour > 23 {
            return Err(SchedulerError::InvalidHour);
        }
        if minute > 59 {
            return Err(SchedulerError::InvalidMinute);
        }
        if !(0..=23 * 60 + 59).contains(&timezone_minutes.abs()) {
            return Err(SchedulerError::InvalidTimezone);
        }
        if missed_grace_ms < 0 {
            return Err(SchedulerError::InvalidGrace);
        }
        let timezone =
            FixedOffset::east_opt(timezone_minutes * 60).ok_or(SchedulerError::InvalidTimezone)?;
        Ok(Self {
            hour,
            minute,
            timezone,
            missed_grace_ms,
        })
    }

    pub fn utc(hour: u8, minute: u8) -> Result<Self, SchedulerError> {
        Self::new(hour, minute, 0, DEFAULT_MISSED_GRACE_MS)
    }

    pub fn decide(
        &self,
        definition_id: &str,
        revision: u64,
        cursor: &SchedulerCursor,
        now_ms: i64,
    ) -> Result<SchedulerDecision, SchedulerError> {
        if definition_id.is_empty() || revision == 0 {
            return Err(SchedulerError::InvalidGrace);
        }
        let now = DateTime::<Utc>::from_timestamp_millis(now_ms)
            .ok_or(SchedulerError::InvalidGrace)?
            .with_timezone(&self.timezone);
        let today = now.date_naive();
        let slot = self.slot(today)?;
        let slot_ms = slot.timestamp_millis();
        let slot_key = slot.to_rfc3339();
        let idempotency_key = format!("{definition_id}:{revision}:{slot_key}");

        if cursor.last_slot.as_deref() == Some(slot_key.as_str()) {
            return Ok(SchedulerDecision::NotDue);
        }
        if now_ms < slot_ms {
            return Ok(SchedulerDecision::NotDue);
        }
        if now_ms.saturating_sub(slot_ms) > self.missed_grace_ms {
            return Ok(SchedulerDecision::Missed {
                slot: slot_key,
                idempotency_key,
            });
        }
        Ok(SchedulerDecision::Trigger {
            slot: slot_key,
            idempotency_key,
        })
    }

    fn slot(&self, date: NaiveDate) -> Result<DateTime<FixedOffset>, SchedulerError> {
        self.timezone
            .with_ymd_and_hms(
                date.year(),
                date.month(),
                date.day(),
                self.hour as u32,
                self.minute as u32,
                0,
            )
            .single()
            .ok_or(SchedulerError::InvalidGrace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at_utc(hour: u32, minute: u32) -> i64 {
        Utc.with_ymd_and_hms(2026, 8, 24, hour, minute, 0)
            .single()
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn emits_one_idempotent_slot_when_due() {
        let schedule = DailySchedule::utc(12, 0).unwrap();
        let decision = schedule
            .decide(
                "daily.sync",
                3,
                &SchedulerCursor { last_slot: None },
                at_utc(12, 2),
            )
            .unwrap();
        assert_eq!(
            decision,
            SchedulerDecision::Trigger {
                slot: "2026-08-24T12:00:00+00:00".into(),
                idempotency_key: "daily.sync:3:2026-08-24T12:00:00+00:00".into()
            }
        );
    }

    #[test]
    fn cursor_fences_duplicate_polling() {
        let schedule = DailySchedule::utc(12, 0).unwrap();
        let cursor = SchedulerCursor {
            last_slot: Some("2026-08-24T12:00:00+00:00".into()),
        };
        assert_eq!(
            schedule
                .decide("daily.sync", 1, &cursor, at_utc(12, 1))
                .unwrap(),
            SchedulerDecision::NotDue
        );
    }

    #[test]
    fn late_poll_is_classified_as_missed_after_grace() {
        let schedule = DailySchedule::new(12, 0, 0, 60_000).unwrap();
        let decision = schedule
            .decide(
                "daily.sync",
                1,
                &SchedulerCursor { last_slot: None },
                at_utc(12, 2),
            )
            .unwrap();
        assert!(matches!(decision, SchedulerDecision::Missed { .. }));
    }

    #[test]
    fn fixed_timezone_selects_the_local_wall_clock_slot() {
        let schedule = DailySchedule::new(15, 0, 180, DEFAULT_MISSED_GRACE_MS).unwrap();
        let decision = schedule
            .decide(
                "daily.sync",
                1,
                &SchedulerCursor { last_slot: None },
                at_utc(12, 1),
            )
            .unwrap();
        assert!(matches!(decision, SchedulerDecision::Trigger { .. }));
    }

    #[test]
    fn rejects_invalid_wall_clock_and_timezone_values() {
        assert_eq!(
            DailySchedule::new(24, 0, 0, 0),
            Err(SchedulerError::InvalidHour)
        );
        assert_eq!(
            DailySchedule::new(0, 60, 0, 0),
            Err(SchedulerError::InvalidMinute)
        );
        assert_eq!(
            DailySchedule::new(0, 0, 24 * 60 + 1, 0),
            Err(SchedulerError::InvalidTimezone)
        );
    }
}
