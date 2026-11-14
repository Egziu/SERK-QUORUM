//! Strongly typed identifiers. Each identifier wraps a UUID so that, for
//! example, a project identifier can never be passed where an organization
//! identifier is expected.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Generates a new time-ordered (UUIDv7) identifier.
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }
            pub const fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }
            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self)
            }
        }

        impl From<$name> for Uuid {
            fn from(id: $name) -> Uuid {
                id.0
            }
        }
    };
}

id_type!(
    /// A person's account.
    UserId
);
id_type!(
    /// A customer organization: the tenant boundary.
    OrganizationId
);
id_type!(ProjectId);
id_type!(DocumentId);
id_type!(DecisionId);
id_type!(InvitationId);
id_type!(SessionId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_through_strings_and_json() {
        let id = ProjectId::new();
        let parsed: ProjectId = id.to_string().parse().expect("parse");
        assert_eq!(id, parsed);
        let json = serde_json::to_string(&id).expect("json");
        assert_eq!(json, format!("\"{id}\""));
    }

    #[test]
    fn new_ids_are_unique_and_time_ordered() {
        let a = DecisionId::new();
        let b = DecisionId::new();
        assert_ne!(a, b);
        assert!(a.as_uuid().get_version_num() == 7);
    }
}
