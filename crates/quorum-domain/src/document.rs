//! Studio documents. A document is a sequence of immutable versions; editing
//! always appends a new version. Approval applies to a specific version.

use crate::{DomainError, validate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentCategory {
    SoftwareRequirements,
    BusinessProposal,
    ProductRequirements,
    ArchitectureDecisionRecord,
    EngineeringReport,
    ResearchBrief,
    ProjectPlan,
    MeetingMinutes,
    ExecutiveSummary,
    StandardOperatingProcedure,
    General,
}

impl DocumentCategory {
    pub const ALL: [DocumentCategory; 11] = [
        Self::SoftwareRequirements,
        Self::BusinessProposal,
        Self::ProductRequirements,
        Self::ArchitectureDecisionRecord,
        Self::EngineeringReport,
        Self::ResearchBrief,
        Self::ProjectPlan,
        Self::MeetingMinutes,
        Self::ExecutiveSummary,
        Self::StandardOperatingProcedure,
        Self::General,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::SoftwareRequirements => "software_requirements",
            Self::BusinessProposal => "business_proposal",
            Self::ProductRequirements => "product_requirements",
            Self::ArchitectureDecisionRecord => "architecture_decision_record",
            Self::EngineeringReport => "engineering_report",
            Self::ResearchBrief => "research_brief",
            Self::ProjectPlan => "project_plan",
            Self::MeetingMinutes => "meeting_minutes",
            Self::ExecutiveSummary => "executive_summary",
            Self::StandardOperatingProcedure => "standard_operating_procedure",
            Self::General => "general",
        }
    }

    pub fn parse(s: &str) -> Result<Self, DomainError> {
        Self::ALL
            .into_iter()
            .find(|c| c.as_str() == s)
            .ok_or_else(|| DomainError::validation("category", "unknown document category"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentStatus {
    /// The latest version has not been approved.
    Draft,
    /// The latest version has been approved by an authorized human.
    Approved,
}

impl DocumentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Approved => "approved",
        }
    }
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        match s {
            "draft" => Ok(Self::Draft),
            "approved" => Ok(Self::Approved),
            _ => Err(DomainError::validation("status", "unknown document status")),
        }
    }
}

pub const TITLE_MAX: usize = 200;
/// 200,000 characters: large specifications fit; request size limits apply.
pub const BODY_MAX: usize = 200_000;
pub const CHANGE_NOTE_MAX: usize = 500;

pub fn title(value: &str) -> Result<String, DomainError> {
    validate::single_line("title", value, 1, TITLE_MAX)
}

pub fn body(value: &str) -> Result<String, DomainError> {
    validate::multi_line("body", value, 0, BODY_MAX)
}

pub fn change_note(value: &str) -> Result<String, DomainError> {
    validate::single_line("change_note", value, 0, CHANGE_NOTE_MAX)
}

/// Renders a version as Markdown for export. AI-assisted content is labelled
/// so the provenance survives outside QUORUM.
pub fn to_markdown(
    title: &str,
    category: DocumentCategory,
    version: i32,
    status: DocumentStatus,
    ai_assisted: bool,
    body: &str,
) -> String {
    let mut out = String::with_capacity(body.len() + 256);
    out.push_str("# ");
    out.push_str(title);
    out.push_str("\n\n");
    out.push_str(&format!(
        "> Category: {} · Version {} · Status: {}\n",
        category.as_str(),
        version,
        status.as_str()
    ));
    if ai_assisted {
        out.push_str(
            "> Contains AI-assisted content. Verify facts and citations before relying on it.\n",
        );
    }
    out.push('\n');
    out.push_str(body);
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_round_trip() {
        for c in DocumentCategory::ALL {
            assert_eq!(DocumentCategory::parse(c.as_str()).expect("parse"), c);
        }
        assert!(DocumentCategory::parse("poem").is_err());
    }

    #[test]
    fn markdown_export_labels_ai_content() {
        let md = to_markdown(
            "Plan",
            DocumentCategory::ProjectPlan,
            3,
            DocumentStatus::Draft,
            true,
            "Body",
        );
        assert!(md.starts_with("# Plan\n"));
        assert!(md.contains("Version 3"));
        assert!(md.contains("AI-assisted"));
        let md = to_markdown(
            "Plan",
            DocumentCategory::ProjectPlan,
            1,
            DocumentStatus::Approved,
            false,
            "Body",
        );
        assert!(!md.contains("AI-assisted"));
    }
}
