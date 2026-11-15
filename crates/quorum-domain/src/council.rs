//! QUORUM Council: structured human review of independently obtained responses.
//!
//! ```text
//! draft ⇄ awaiting_responses ⇄ ready_for_review ⇄ under_review ──record──▶ decision_recorded ──▶ archived
//! ```
//!
//! Model agreement is not verification: Council records human judgement
//! (annotations, criterion ratings with reasons, a decision summary). It never
//! computes a confidence score.

use crate::{DomainError, Permission, validate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CouncilStatus {
    Draft,
    AwaitingResponses,
    ReadyForReview,
    UnderReview,
    DecisionRecorded,
    Archived,
}

impl CouncilStatus {
    pub const ALL: [CouncilStatus; 6] = [
        Self::Draft,
        Self::AwaitingResponses,
        Self::ReadyForReview,
        Self::UnderReview,
        Self::DecisionRecorded,
        Self::Archived,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::AwaitingResponses => "awaiting_responses",
            Self::ReadyForReview => "ready_for_review",
            Self::UnderReview => "under_review",
            Self::DecisionRecorded => "decision_recorded",
            Self::Archived => "archived",
        }
    }

    pub fn parse(s: &str) -> Result<Self, DomainError> {
        Self::ALL
            .into_iter()
            .find(|c| c.as_str() == s)
            .ok_or_else(|| DomainError::validation("status", "unknown council status"))
    }

    /// Responses, annotations and ratings may change only before a decision.
    pub fn is_open(self) -> bool {
        !matches!(self, Self::DecisionRecorded | Self::Archived)
    }

    /// Validates a transition and returns the permission it needs.
    /// Recording the decision is a separate command (`record_decision`).
    pub fn transition(self, to: CouncilStatus) -> Result<Permission, DomainError> {
        use CouncilStatus::*;
        match (self, to) {
            (Draft, AwaitingResponses)
            | (AwaitingResponses, Draft)
            | (AwaitingResponses, ReadyForReview)
            | (ReadyForReview, AwaitingResponses)
            | (ReadyForReview, UnderReview)
            | (UnderReview, ReadyForReview) => Ok(Permission::DecisionsWrite),
            (DecisionRecorded, Archived) | (Draft, Archived) => Ok(Permission::DecisionsDecide),
            _ => Err(DomainError::InvalidTransition {
                from: self.as_str(),
                to: to.as_str(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    Note,
    Highlight,
    Evidence,
    Assumption,
    Contradiction,
    Alternative,
    Question,
    Risk,
}

impl AnnotationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Highlight => "highlight",
            Self::Evidence => "evidence",
            Self::Assumption => "assumption",
            Self::Contradiction => "contradiction",
            Self::Alternative => "alternative",
            Self::Question => "question",
            Self::Risk => "risk",
        }
    }
}

/// Transparent, human-rated evaluation criteria.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Criterion {
    EvidenceQuality,
    RequirementsCoverage,
    TechnicalFeasibility,
    Clarity,
    CostImplications,
    Maintainability,
    IdentifiedRisks,
    VerificationStatus,
}

impl Criterion {
    pub const ALL: [Criterion; 8] = [
        Self::EvidenceQuality,
        Self::RequirementsCoverage,
        Self::TechnicalFeasibility,
        Self::Clarity,
        Self::CostImplications,
        Self::Maintainability,
        Self::IdentifiedRisks,
        Self::VerificationStatus,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::EvidenceQuality => "evidence_quality",
            Self::RequirementsCoverage => "requirements_coverage",
            Self::TechnicalFeasibility => "technical_feasibility",
            Self::Clarity => "clarity",
            Self::CostImplications => "cost_implications",
            Self::Maintainability => "maintainability",
            Self::IdentifiedRisks => "identified_risks",
            Self::VerificationStatus => "verification_status",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::EvidenceQuality => "Evidence quality",
            Self::RequirementsCoverage => "Requirements coverage",
            Self::TechnicalFeasibility => "Technical feasibility",
            Self::Clarity => "Clarity",
            Self::CostImplications => "Cost implications",
            Self::Maintainability => "Maintainability",
            Self::IdentifiedRisks => "Identified risks",
            Self::VerificationStatus => "Verification status",
        }
    }
}

pub fn title(value: &str) -> Result<String, DomainError> {
    validate::single_line("title", value, 1, 200)
}

pub fn question(value: &str) -> Result<String, DomainError> {
    validate::multi_line("question", value, 0, 8_000)
}

pub fn decision_summary(value: &str) -> Result<String, DomainError> {
    validate::multi_line("decision_summary", value, 1, 20_000)
}

pub fn rating_score(score: i16) -> Result<i16, DomainError> {
    if (1..=5).contains(&score) {
        Ok(score)
    } else {
        Err(DomainError::validation("score", "must be between 1 and 5"))
    }
}

#[cfg(test)]
mod tests {
    use super::CouncilStatus::*;
    use super::*;

    #[test]
    fn allowed_and_forbidden_transitions() {
        assert_eq!(
            Draft.transition(AwaitingResponses),
            Ok(Permission::DecisionsWrite)
        );
        assert_eq!(
            ReadyForReview.transition(UnderReview),
            Ok(Permission::DecisionsWrite)
        );
        assert_eq!(
            DecisionRecorded.transition(Archived),
            Ok(Permission::DecisionsDecide)
        );
        for to in CouncilStatus::ALL {
            assert!(Archived.transition(to).is_err(), "archived is final");
            assert!(
                UnderReview.transition(DecisionRecorded).is_err(),
                "use record_decision"
            );
        }
        assert!(DecisionRecorded.transition(UnderReview).is_err());
        assert!(Draft.transition(UnderReview).is_err());
    }

    #[test]
    fn open_states() {
        assert!(Draft.is_open() && UnderReview.is_open());
        assert!(!DecisionRecorded.is_open() && !Archived.is_open());
    }

    #[test]
    fn scores_and_round_trips() {
        assert!(rating_score(0).is_err() && rating_score(6).is_err());
        assert_eq!(rating_score(3), Ok(3));
        for s in CouncilStatus::ALL {
            assert_eq!(CouncilStatus::parse(s.as_str()).expect("parse"), s);
        }
        assert_eq!(Criterion::ALL.len(), 8);
    }
}
