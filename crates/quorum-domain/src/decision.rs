//! Decision records and their lifecycle.
//!
//! ```text
//!   draft ──submit──▶ under_review ──approve──▶ approved ──supersede──▶ superseded
//!     ▲                    │  │
//!     └────return──────────┘  └──reject──▶ rejected ──reopen──▶ draft
//! ```
//!
//! Content may be edited only in `draft`. Approved, rejected and superseded
//! records are never overwritten; every change appends a version.

use crate::{DomainError, Permission, validate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    Draft,
    UnderReview,
    Approved,
    Rejected,
    Superseded,
}

impl DecisionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::UnderReview => "under_review",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Superseded => "superseded",
        }
    }

    pub fn parse(s: &str) -> Result<Self, DomainError> {
        match s {
            "draft" => Ok(Self::Draft),
            "under_review" => Ok(Self::UnderReview),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "superseded" => Ok(Self::Superseded),
            _ => Err(DomainError::validation("status", "unknown decision status")),
        }
    }

    pub fn is_editable(self) -> bool {
        self == Self::Draft
    }

    /// Validates a status transition and returns the permission it requires.
    /// Submitting and returning a draft is authoring work; approving,
    /// rejecting and superseding are decisions reserved to deciders.
    pub fn transition(self, to: DecisionStatus) -> Result<Permission, DomainError> {
        use DecisionStatus::*;
        match (self, to) {
            (Draft, UnderReview) | (UnderReview, Draft) | (Rejected, Draft) => {
                Ok(Permission::DecisionsWrite)
            }
            (UnderReview, Approved) | (UnderReview, Rejected) | (Approved, Superseded) => {
                Ok(Permission::DecisionsDecide)
            }
            _ => Err(DomainError::InvalidTransition {
                from: self.as_str(),
                to: to.as_str(),
            }),
        }
    }
}

/// The editable content of a decision record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DecisionContent {
    pub title: String,
    pub problem_statement: String,
    pub context: String,
    pub alternatives: String,
    pub evaluation_criteria: String,
    pub evidence: String,
    pub outcome: String,
    pub consequences: String,
    pub risks: String,
}

pub const TITLE_MAX: usize = 200;
pub const FIELD_MAX: usize = 20_000;

impl DecisionContent {
    /// Validates and normalizes every field.
    pub fn validated(self) -> Result<Self, DomainError> {
        let field = |name, v: &str| validate::multi_line(name, v, 0, FIELD_MAX);
        Ok(Self {
            title: validate::single_line("title", &self.title, 1, TITLE_MAX)?,
            problem_statement: validate::multi_line(
                "problem_statement",
                &self.problem_statement,
                1,
                FIELD_MAX,
            )?,
            context: field("context", &self.context)?,
            alternatives: field("alternatives", &self.alternatives)?,
            evaluation_criteria: field("evaluation_criteria", &self.evaluation_criteria)?,
            evidence: field("evidence", &self.evidence)?,
            outcome: field("outcome", &self.outcome)?,
            consequences: field("consequences", &self.consequences)?,
            risks: field("risks", &self.risks)?,
        })
    }

    /// A decision may be submitted for review only once it states an outcome
    /// and the alternatives that were considered.
    pub fn ready_for_review(&self) -> Result<(), DomainError> {
        if self.outcome.trim().is_empty() {
            return Err(DomainError::validation("outcome", "required before review"));
        }
        if self.alternatives.trim().is_empty() {
            return Err(DomainError::validation(
                "alternatives",
                "required before review",
            ));
        }
        Ok(())
    }
}

pub fn format_reference(number: i32) -> String {
    format!("DEC-{number:04}")
}

#[cfg(test)]
mod tests {
    use super::DecisionStatus::*;
    use super::*;

    const ALL: [DecisionStatus; 5] = [Draft, UnderReview, Approved, Rejected, Superseded];

    #[test]
    fn allowed_transitions_and_required_permissions() {
        assert_eq!(
            Draft.transition(UnderReview),
            Ok(Permission::DecisionsWrite)
        );
        assert_eq!(
            UnderReview.transition(Draft),
            Ok(Permission::DecisionsWrite)
        );
        assert_eq!(
            UnderReview.transition(Approved),
            Ok(Permission::DecisionsDecide)
        );
        assert_eq!(
            UnderReview.transition(Rejected),
            Ok(Permission::DecisionsDecide)
        );
        assert_eq!(
            Approved.transition(Superseded),
            Ok(Permission::DecisionsDecide)
        );
        assert_eq!(Rejected.transition(Draft), Ok(Permission::DecisionsWrite));
    }

    #[test]
    fn everything_else_is_rejected() {
        let allowed = [
            (Draft, UnderReview),
            (UnderReview, Draft),
            (UnderReview, Approved),
            (UnderReview, Rejected),
            (Approved, Superseded),
            (Rejected, Draft),
        ];
        for from in ALL {
            for to in ALL {
                if !allowed.contains(&(from, to)) {
                    assert!(
                        from.transition(to).is_err(),
                        "{from:?} -> {to:?} must be rejected"
                    );
                }
            }
        }
    }

    #[test]
    fn approved_and_terminal_states_are_not_editable() {
        assert!(Draft.is_editable());
        for s in [UnderReview, Approved, Rejected, Superseded] {
            assert!(!s.is_editable());
        }
    }

    #[test]
    fn content_validation() {
        let ok = DecisionContent {
            title: " Use PostgreSQL ".into(),
            problem_statement: "Need a transactional store".into(),
            ..Default::default()
        }
        .validated()
        .expect("valid");
        assert_eq!(ok.title, "Use PostgreSQL");
        assert!(ok.ready_for_review().is_err());
        let ready = DecisionContent {
            outcome: "PostgreSQL 16".into(),
            alternatives: "MySQL; SQLite".into(),
            ..ok
        };
        assert!(ready.ready_for_review().is_ok());
        assert!(
            DecisionContent {
                title: "x".into(),
                ..Default::default()
            }
            .validated()
            .is_err(),
            "problem statement is required"
        );
    }

    #[test]
    fn reference_format() {
        assert_eq!(format_reference(7), "DEC-0007");
        assert_eq!(format_reference(12345), "DEC-12345");
    }
}
