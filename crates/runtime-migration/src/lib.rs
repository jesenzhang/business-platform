//! Shared embedded migration catalog and runtime compatibility rules.

use sqlx::migrate::Migrator;

/// The sole embedded catalog used by migration and runtime readiness.
pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationCompatibility {
    Empty,
    Behind,
    Equal,
    Ahead,
}

impl MigrationCompatibility {
    #[must_use]
    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Equal)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Behind => "behind",
            Self::Equal => "compatible",
            Self::Ahead => "ahead",
        }
    }
}

#[must_use]
pub fn latest_version() -> i64 {
    MIGRATOR
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or(0)
}

#[must_use]
pub fn classify(applied_version: i64) -> MigrationCompatibility {
    if applied_version <= 0 {
        MigrationCompatibility::Empty
    } else {
        match applied_version.cmp(&latest_version()) {
            std::cmp::Ordering::Less => MigrationCompatibility::Behind,
            std::cmp::Ordering::Equal => MigrationCompatibility::Equal,
            std::cmp::Ordering::Greater => MigrationCompatibility::Ahead,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_empty_behind_equal_and_ahead() {
        let latest = latest_version();
        assert!(latest > 1);
        assert_eq!(classify(0), MigrationCompatibility::Empty);
        assert_eq!(classify(latest - 1), MigrationCompatibility::Behind);
        assert_eq!(classify(latest), MigrationCompatibility::Equal);
        assert_eq!(classify(latest + 1), MigrationCompatibility::Ahead);
        assert!(classify(latest).is_ready());
        assert!(!classify(latest + 1).is_ready());
    }
}
