#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use std::time::Instant;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use web_time::Instant;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Milliseconds on the stack's logical timeline.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct LogicalTime(i64);

impl LogicalTime {
    pub const UNIX_EPOCH: Self = Self(0);

    #[must_use]
    pub const fn from_unix_millis(value: i64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_unix_millis(self) -> i64 {
        self.0
    }

    pub fn checked_add(self, millis: u64) -> std::result::Result<Self, ClockError> {
        let millis = i64::try_from(millis).map_err(|_| ClockError::Overflow)?;
        self.0
            .checked_add(millis)
            .map(Self)
            .ok_or(ClockError::Overflow)
    }
}

/// Whether time follows the host or advances only under test control.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockMode {
    Manual,
    Realtime,
}

/// Serializable clock state used by stack snapshots.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockSnapshot {
    pub mode: ClockMode,
    pub now: LogicalTime,
}

/// One authoritative timeline for every resource in a stack.
#[derive(Clone, Debug)]
pub struct Clock {
    mode: ClockMode,
    base: LogicalTime,
    anchor: Option<Instant>,
}

impl Clock {
    #[must_use]
    pub fn manual(start: LogicalTime) -> Self {
        Self {
            mode: ClockMode::Manual,
            base: start,
            anchor: None,
        }
    }

    #[must_use]
    pub fn realtime(start: LogicalTime) -> Self {
        Self {
            mode: ClockMode::Realtime,
            base: start,
            anchor: Some(Instant::now()),
        }
    }

    #[must_use]
    pub const fn mode(&self) -> ClockMode {
        self.mode
    }

    pub fn now(&self) -> std::result::Result<LogicalTime, ClockError> {
        match self.mode {
            ClockMode::Manual => Ok(self.base),
            ClockMode::Realtime => {
                let elapsed = u64::try_from(
                    self.anchor
                        .as_ref()
                        .map_or(0, |anchor| anchor.elapsed().as_millis()),
                )
                .map_err(|_| ClockError::Overflow)?;
                self.base.checked_add(elapsed)
            }
        }
    }

    pub fn advance(&mut self, millis: u64) -> std::result::Result<LogicalTime, ClockError> {
        if self.mode != ClockMode::Manual {
            return Err(ClockError::RealtimeAdvance);
        }
        self.base = self.base.checked_add(millis)?;
        Ok(self.base)
    }

    pub fn set(&mut self, now: LogicalTime) {
        self.base = now;
        self.anchor = (self.mode == ClockMode::Realtime).then(Instant::now);
    }

    pub fn snapshot(&self) -> std::result::Result<ClockSnapshot, ClockError> {
        Ok(ClockSnapshot {
            mode: self.mode,
            now: self.now()?,
        })
    }

    #[must_use]
    pub fn from_snapshot(snapshot: ClockSnapshot) -> Self {
        match snapshot.mode {
            ClockMode::Manual => Self::manual(snapshot.now),
            ClockMode::Realtime => Self::realtime(snapshot.now),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ClockError {
    #[error("logical time overflowed")]
    Overflow,
    #[error("a realtime clock cannot be advanced manually")]
    RealtimeAdvance,
}

#[cfg(test)]
mod tests {
    use super::{Clock, ClockError, LogicalTime};

    #[test]
    fn manual_time_advances_without_waiting() {
        let mut clock = Clock::manual(LogicalTime::from_unix_millis(1_000));
        assert_eq!(
            clock.advance(250).map(LogicalTime::as_unix_millis),
            Ok(1_250)
        );
        assert_eq!(clock.now().map(LogicalTime::as_unix_millis), Ok(1_250));
    }

    #[test]
    fn realtime_time_rejects_manual_advancement() {
        let mut clock = Clock::realtime(LogicalTime::UNIX_EPOCH);
        assert_eq!(clock.advance(1), Err(ClockError::RealtimeAdvance));
    }
}
