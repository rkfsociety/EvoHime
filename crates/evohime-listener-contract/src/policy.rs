use crate::error::ContractError;
use serde::{Deserialize, Serialize};

pub const MAX_QUIET_HOURS: usize = 16;
pub const MAX_BLOCKLIST_ENTRIES: usize = 64;
pub const MAX_PATTERN_BYTES: usize = 128;
/// Patterns are globs, not regular expressions; a pattern that is mostly
/// wildcards matches everything and is a configuration mistake, not a filter.
pub const MAX_PATTERN_WILDCARDS: usize = 8;
pub const MAX_RETENTION_DAYS: u32 = 90;
pub const DEFAULT_RETENTION_DAYS: u32 = 7;
pub const MINUTES_PER_DAY: u32 = 1440;

/// Half-open quiet window `[start_minute, end_minute)` in local minutes of the
/// day.  A window may wrap midnight; an empty window is rejected because a
/// user who configured quiet hours meant a non-empty period.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuietHours {
    pub start_minute: u32,
    pub end_minute: u32,
}

impl QuietHours {
    pub fn new(start_minute: u32, end_minute: u32) -> Result<Self, ContractError> {
        let window = Self {
            start_minute,
            end_minute,
        };
        window.validate()?;
        Ok(window)
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        if self.start_minute >= MINUTES_PER_DAY
            || self.end_minute >= MINUTES_PER_DAY
            || self.start_minute == self.end_minute
        {
            return Err(ContractError::InvalidQuietHours);
        }
        Ok(())
    }

    pub const fn contains(&self, minute_of_day: u32) -> bool {
        if self.start_minute <= self.end_minute {
            minute_of_day >= self.start_minute && minute_of_day < self.end_minute
        } else {
            minute_of_day >= self.start_minute || minute_of_day < self.end_minute
        }
    }
}

/// Ambient policy, version 1.
///
/// During quiet hours the capture stream is closed completely: nothing is
/// recognized and nothing is stored.  The same holds for `paused` and for a
/// blocklist match — filtering happens before audio exists, not after.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AmbientPolicy {
    /// User-held pause.  Survives restarts, unlike a transient state.
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub quiet_hours: Vec<QuietHours>,
    /// Glob patterns matched against the foreground process name.
    #[serde(default)]
    pub process_blocklist: Vec<String>,
    /// Glob patterns matched against the foreground window title.
    #[serde(default)]
    pub window_title_blocklist: Vec<String>,
    /// Retention of transcript text, in days.
    pub retention_days: u32,
    /// Распознавать ли адресованные Еве голосовые команды («Ева, открой …»).
    ///
    /// Выключение возвращает слушание к прежнему поведению: фраза остаётся
    /// обычным транскриптом и ничего не запускает.
    #[serde(default = "enabled_by_default")]
    pub voice_commands: bool,
    /// Запускать распознанную команду без подтверждения.
    ///
    /// По умолчанию `false`: услышанная команда показывает карточку, и
    /// приложение открывает клик, а не микрофон. Значение `true` — осознанный
    /// выбор пользователя, а не то, во что настройка сползает сама.
    #[serde(default)]
    pub voice_commands_autorun: bool,
}

const fn enabled_by_default() -> bool {
    true
}

impl Default for AmbientPolicy {
    fn default() -> Self {
        Self {
            paused: false,
            quiet_hours: Vec::new(),
            process_blocklist: Vec::new(),
            window_title_blocklist: Vec::new(),
            retention_days: DEFAULT_RETENTION_DAYS,
            voice_commands: true,
            voice_commands_autorun: false,
        }
    }
}

impl AmbientPolicy {
    /// Full validation.  A policy is applied only after it passes: an invalid
    /// policy is rejected as a whole, it never degrades into "listen to
    /// everything".
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.quiet_hours.len() > MAX_QUIET_HOURS {
            return Err(ContractError::TooManyEntries("quiet_hours"));
        }
        for window in &self.quiet_hours {
            window.validate()?;
        }
        if self.process_blocklist.len() + self.window_title_blocklist.len() > MAX_BLOCKLIST_ENTRIES
        {
            return Err(ContractError::TooManyEntries("blocklists"));
        }
        validate_blocklist(&self.process_blocklist, "process_blocklist")?;
        validate_blocklist(&self.window_title_blocklist, "window_title_blocklist")?;
        if self.retention_days == 0 || self.retention_days > MAX_RETENTION_DAYS {
            return Err(ContractError::RetentionOutOfBounds);
        }
        Ok(())
    }

    pub fn is_quiet_at(&self, minute_of_day: u32) -> bool {
        self.quiet_hours
            .iter()
            .any(|window| window.contains(minute_of_day))
    }

    /// True when the policy alone permits an open capture stream.  Capability,
    /// device state and blocklist matching are checked separately by the
    /// listener; this answers only the policy half.
    pub fn capture_allowed_at(&self, minute_of_day: u32) -> bool {
        !self.paused && !self.is_quiet_at(minute_of_day)
    }
}

fn validate_blocklist(patterns: &[String], field: &'static str) -> Result<(), ContractError> {
    if patterns.len() > MAX_BLOCKLIST_ENTRIES {
        return Err(ContractError::TooManyEntries(field));
    }
    for pattern in patterns {
        validate_pattern(pattern, field)?;
    }
    Ok(())
}

/// Blocklist patterns are globs with `*` and `?`.  Regular-expression
/// metacharacters are rejected outright: they would either be matched
/// literally (a silent hole in the blocklist) or, in a future matcher, open
/// the door to catastrophic backtracking.
fn validate_pattern(pattern: &str, field: &'static str) -> Result<(), ContractError> {
    if pattern.trim().is_empty() {
        return Err(ContractError::EmptyField(field));
    }
    if pattern.len() > MAX_PATTERN_BYTES {
        return Err(ContractError::FieldTooLong(field));
    }
    let mut wildcards = 0usize;
    for ch in pattern.chars() {
        if ch.is_control() {
            return Err(ContractError::InvalidCharacter(field));
        }
        match ch {
            '*' | '?' => wildcards += 1,
            '(' | ')' | '[' | ']' | '{' | '}' | '|' | '+' | '^' | '$' | '\\' => {
                return Err(ContractError::InvalidCharacter(field))
            }
            _ => {}
        }
    }
    if wildcards > MAX_PATTERN_WILDCARDS {
        return Err(ContractError::PatternTooComplex(field));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy_with_patterns(patterns: Vec<String>) -> AmbientPolicy {
        AmbientPolicy {
            process_blocklist: patterns,
            ..AmbientPolicy::default()
        }
    }

    #[test]
    fn default_policy_is_valid_and_listens() {
        let policy = AmbientPolicy::default();
        assert_eq!(policy.validate(), Ok(()));
        assert_eq!(policy.retention_days, DEFAULT_RETENTION_DAYS);
        assert!(policy.capture_allowed_at(0));
    }

    #[test]
    fn quiet_hours_close_the_stream_and_may_wrap_midnight() {
        let policy = AmbientPolicy {
            quiet_hours: vec![QuietHours::new(23 * 60, 7 * 60).unwrap()],
            ..AmbientPolicy::default()
        };
        assert_eq!(policy.validate(), Ok(()));
        assert!(policy.is_quiet_at(23 * 60));
        assert!(policy.is_quiet_at(3 * 60));
        assert!(!policy.is_quiet_at(7 * 60));
        assert!(!policy.capture_allowed_at(2 * 60));
        assert!(policy.capture_allowed_at(12 * 60));
    }

    #[test]
    fn pause_closes_the_stream_at_any_hour() {
        let policy = AmbientPolicy {
            paused: true,
            ..AmbientPolicy::default()
        };
        for minute in 0..MINUTES_PER_DAY {
            assert!(!policy.capture_allowed_at(minute));
        }
    }

    #[test]
    fn invalid_quiet_hours_are_rejected() {
        assert_eq!(
            QuietHours::new(60, 60),
            Err(ContractError::InvalidQuietHours)
        );
        assert_eq!(
            QuietHours::new(0, MINUTES_PER_DAY),
            Err(ContractError::InvalidQuietHours)
        );
        let policy = AmbientPolicy {
            quiet_hours: vec![QuietHours {
                start_minute: 5000,
                end_minute: 10,
            }],
            ..AmbientPolicy::default()
        };
        assert_eq!(policy.validate(), Err(ContractError::InvalidQuietHours));
    }

    #[test]
    fn oversized_policy_is_rejected_before_it_is_applied() {
        let too_many = AmbientPolicy {
            quiet_hours: vec![QuietHours::new(0, 10).unwrap(); MAX_QUIET_HOURS + 1],
            ..AmbientPolicy::default()
        };
        assert_eq!(
            too_many.validate(),
            Err(ContractError::TooManyEntries("quiet_hours"))
        );

        let long_list =
            policy_with_patterns(vec!["chrome.exe".to_string(); MAX_BLOCKLIST_ENTRIES + 1]);
        assert_eq!(
            long_list.validate(),
            Err(ContractError::TooManyEntries("blocklists"))
        );

        let split_list = AmbientPolicy {
            process_blocklist: vec!["process-*".to_string(); MAX_BLOCKLIST_ENTRIES / 2],
            window_title_blocklist: vec![
                "title-*".to_string();
                MAX_BLOCKLIST_ENTRIES - MAX_BLOCKLIST_ENTRIES / 2 + 1
            ],
            ..AmbientPolicy::default()
        };
        assert_eq!(
            split_list.validate(),
            Err(ContractError::TooManyEntries("blocklists"))
        );

        let long_pattern = policy_with_patterns(vec!["a".repeat(MAX_PATTERN_BYTES + 1)]);
        assert_eq!(
            long_pattern.validate(),
            Err(ContractError::FieldTooLong("process_blocklist"))
        );
    }

    #[test]
    fn regex_shaped_patterns_are_rejected() {
        for pattern in ["(a+)+b", "^bank.*$", "a|b", "chrome[.]exe", "a\\b"] {
            let policy = policy_with_patterns(vec![pattern.to_string()]);
            assert_eq!(
                policy.validate(),
                Err(ContractError::InvalidCharacter("process_blocklist")),
                "accepted regex-shaped pattern {pattern}"
            );
        }
        let excessive = policy_with_patterns(vec!["*".repeat(MAX_PATTERN_WILDCARDS + 1)]);
        assert_eq!(
            excessive.validate(),
            Err(ContractError::PatternTooComplex("process_blocklist"))
        );
        let empty = AmbientPolicy {
            window_title_blocklist: vec!["   ".to_string()],
            ..AmbientPolicy::default()
        };
        assert_eq!(
            empty.validate(),
            Err(ContractError::EmptyField("window_title_blocklist"))
        );
    }

    #[test]
    fn retention_beyond_ninety_days_is_rejected() {
        for days in [0, MAX_RETENTION_DAYS + 1, 365] {
            let policy = AmbientPolicy {
                retention_days: days,
                ..AmbientPolicy::default()
            };
            assert_eq!(policy.validate(), Err(ContractError::RetentionOutOfBounds));
        }
        let policy = AmbientPolicy {
            retention_days: MAX_RETENTION_DAYS,
            ..AmbientPolicy::default()
        };
        assert_eq!(policy.validate(), Ok(()));
    }
}
