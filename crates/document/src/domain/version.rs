//! Strongly typed document versions.

use std::fmt;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AggregateVersion(i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentRevision(i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("version must be positive")]
pub struct InvalidVersionValue;

impl AggregateVersion {
    pub fn new(value: i64) -> Result<Self, InvalidVersionValue> {
        if value > 0 {
            Ok(Self(value))
        } else {
            Err(InvalidVersionValue)
        }
    }

    #[must_use]
    pub const fn value(self) -> i64 {
        self.0
    }

    pub fn increment(self) -> Result<Self, InvalidVersionValue> {
        self.0
            .checked_add(1)
            .and_then(|value| Self::new(value).ok())
            .ok_or(InvalidVersionValue)
    }
}

impl ContentRevision {
    pub fn new(value: i64) -> Result<Self, InvalidVersionValue> {
        if value > 0 {
            Ok(Self(value))
        } else {
            Err(InvalidVersionValue)
        }
    }

    #[must_use]
    pub const fn value(self) -> i64 {
        self.0
    }

    pub fn increment(self) -> Result<Self, InvalidVersionValue> {
        self.0
            .checked_add(1)
            .and_then(|value| Self::new(value).ok())
            .ok_or(InvalidVersionValue)
    }
}

impl fmt::Display for AggregateVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl fmt::Display for ContentRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_reject_non_positive_values_and_increment() {
        assert!(AggregateVersion::new(0).is_err());
        assert!(ContentRevision::new(-1).is_err());
        assert_eq!(
            AggregateVersion::new(1)
                .unwrap_or_else(|_| unreachable!())
                .increment()
                .unwrap_or_else(|_| unreachable!())
                .value(),
            2
        );
        assert_eq!(
            ContentRevision::new(1)
                .unwrap_or_else(|_| unreachable!())
                .increment()
                .unwrap_or_else(|_| unreachable!())
                .value(),
            2
        );
    }
}
