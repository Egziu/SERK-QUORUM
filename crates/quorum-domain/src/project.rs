use crate::{DomainError, validate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Active,
    OnHold,
    Completed,
    Archived,
}

impl ProjectStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectStatus::Active => "active",
            ProjectStatus::OnHold => "on_hold",
            ProjectStatus::Completed => "completed",
            ProjectStatus::Archived => "archived",
        }
    }

    pub fn parse(s: &str) -> Result<Self, DomainError> {
        match s {
            "active" => Ok(Self::Active),
            "on_hold" => Ok(Self::OnHold),
            "completed" => Ok(Self::Completed),
            "archived" => Ok(Self::Archived),
            _ => Err(DomainError::validation("status", "unknown project status")),
        }
    }

    /// Archived projects are read-only: documents and decisions inside them
    /// cannot be created or changed until the project is reactivated.
    pub fn accepts_changes(self) -> bool {
        self != ProjectStatus::Archived
    }
}

pub const NAME_MAX: usize = 120;
pub const DESCRIPTION_MAX: usize = 4_000;

pub fn name(value: &str) -> Result<String, DomainError> {
    validate::single_line("name", value, 1, NAME_MAX)
}

pub fn description(value: &str) -> Result<String, DomainError> {
    validate::multi_line("description", value, 0, DESCRIPTION_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trip_and_archive_rule() {
        for s in [
            ProjectStatus::Active,
            ProjectStatus::OnHold,
            ProjectStatus::Completed,
            ProjectStatus::Archived,
        ] {
            assert_eq!(ProjectStatus::parse(s.as_str()).expect("parse"), s);
        }
        assert!(!ProjectStatus::Archived.accepts_changes());
        assert!(ProjectStatus::Completed.accepts_changes());
        assert!(ProjectStatus::parse("deleted").is_err());
    }
}
