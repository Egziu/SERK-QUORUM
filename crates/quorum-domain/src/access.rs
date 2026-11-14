//! Organization roles and the permissions they grant.
//!
//! Authorization is deny-by-default: a role grants exactly the permissions
//! listed in [`Role::permissions`]. The mapping is the single source of truth
//! used by the API; the web client receives it only to hide controls, never
//! to enforce access.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Administrator,
    ProjectManager,
    Contributor,
    Reviewer,
    BillingAdministrator,
    Auditor,
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// See the organization, its projects, documents and decisions.
    WorkspaceRead,
    /// Invite members and change roles (except the owner role).
    MembersManage,
    /// Create, edit and archive projects.
    ProjectsManage,
    /// Create documents and new document versions.
    DocumentsWrite,
    /// Approve a document version.
    DocumentsApprove,
    /// Create and edit draft decision records and submit them for review.
    DecisionsWrite,
    /// Approve, reject or supersede decision records.
    DecisionsDecide,
    /// Submit text to an AI provider (fixture or live).
    ProvidersUse,
    /// Read the organization's audit trail.
    AuditRead,
    /// Read usage records.
    UsageRead,
    /// Manage subscription and billing (reserved; no billing yet).
    BillingManage,
}

impl Role {
    pub const ALL: [Role; 8] = [
        Role::Owner,
        Role::Administrator,
        Role::ProjectManager,
        Role::Contributor,
        Role::Reviewer,
        Role::BillingAdministrator,
        Role::Auditor,
        Role::ReadOnly,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Administrator => "administrator",
            Role::ProjectManager => "project_manager",
            Role::Contributor => "contributor",
            Role::Reviewer => "reviewer",
            Role::BillingAdministrator => "billing_administrator",
            Role::Auditor => "auditor",
            Role::ReadOnly => "read_only",
        }
    }

    pub fn permissions(self) -> &'static [Permission] {
        use Permission::*;
        match self {
            Role::Owner | Role::Administrator => &[
                WorkspaceRead,
                MembersManage,
                ProjectsManage,
                DocumentsWrite,
                DocumentsApprove,
                DecisionsWrite,
                DecisionsDecide,
                ProvidersUse,
                AuditRead,
                UsageRead,
                BillingManage,
            ],
            Role::ProjectManager => &[
                WorkspaceRead,
                ProjectsManage,
                DocumentsWrite,
                DocumentsApprove,
                DecisionsWrite,
                DecisionsDecide,
                ProvidersUse,
                UsageRead,
            ],
            Role::Contributor => &[WorkspaceRead, DocumentsWrite, DecisionsWrite, ProvidersUse],
            Role::Reviewer => &[WorkspaceRead, DocumentsApprove, DecisionsDecide],
            Role::BillingAdministrator => &[WorkspaceRead, UsageRead, BillingManage],
            Role::Auditor => &[WorkspaceRead, AuditRead, UsageRead],
            Role::ReadOnly => &[WorkspaceRead],
        }
    }

    pub fn allows(self, permission: Permission) -> bool {
        self.permissions().contains(&permission)
    }

    /// Whether a member holding `self` may assign `target` to someone.
    /// Only owners may grant or revoke the owner role.
    pub fn may_assign(self, target: Role) -> bool {
        match self {
            Role::Owner => true,
            Role::Administrator => target != Role::Owner,
            _ => false,
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Role {
    type Err = crate::DomainError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Role::ALL
            .into_iter()
            .find(|r| r.as_str() == s)
            .ok_or_else(|| crate::DomainError::validation("role", "unknown role"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Permission::*;

    #[test]
    fn role_strings_round_trip() {
        for role in Role::ALL {
            assert_eq!(role.as_str().parse::<Role>().expect("parse"), role);
        }
        assert!("superuser".parse::<Role>().is_err());
    }

    #[test]
    fn every_role_can_read_the_workspace() {
        for role in Role::ALL {
            assert!(role.allows(WorkspaceRead), "{role}");
        }
    }

    #[test]
    fn read_only_and_auditor_cannot_write() {
        for role in [Role::ReadOnly, Role::Auditor, Role::BillingAdministrator] {
            for p in [
                ProjectsManage,
                DocumentsWrite,
                DecisionsWrite,
                DecisionsDecide,
                ProvidersUse,
            ] {
                assert!(!role.allows(p), "{role} must not have {p:?}");
            }
        }
    }

    #[test]
    fn only_audit_roles_read_audit() {
        let readers: Vec<Role> = Role::ALL
            .into_iter()
            .filter(|r| r.allows(AuditRead))
            .collect();
        assert_eq!(
            readers,
            vec![Role::Owner, Role::Administrator, Role::Auditor]
        );
    }

    #[test]
    fn reviewer_decides_but_does_not_author() {
        assert!(Role::Reviewer.allows(DecisionsDecide));
        assert!(!Role::Reviewer.allows(DecisionsWrite));
        assert!(Role::Contributor.allows(DecisionsWrite));
        assert!(!Role::Contributor.allows(DecisionsDecide));
    }

    #[test]
    fn owner_role_assignment_is_restricted() {
        assert!(Role::Owner.may_assign(Role::Owner));
        assert!(!Role::Administrator.may_assign(Role::Owner));
        assert!(Role::Administrator.may_assign(Role::Contributor));
        for role in [
            Role::ProjectManager,
            Role::Contributor,
            Role::Reviewer,
            Role::ReadOnly,
        ] {
            assert!(!role.may_assign(Role::ReadOnly));
        }
    }
}
