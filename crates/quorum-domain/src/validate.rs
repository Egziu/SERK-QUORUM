//! Input validation applied at the API trust boundary.

use crate::DomainError;

/// Validates a single-line text field: trims surrounding whitespace, checks
/// length in characters and rejects control characters.
pub fn single_line(
    field: &'static str,
    value: &str,
    min: usize,
    max: usize,
) -> Result<String, DomainError> {
    let value = value.trim();
    let len = value.chars().count();
    if len < min || len > max {
        return Err(DomainError::validation(
            field,
            format!("must be between {min} and {max} characters"),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(DomainError::validation(
            field,
            "must not contain control characters",
        ));
    }
    Ok(value.to_owned())
}

/// Validates a multi-line text body. Newlines and tabs are allowed; other
/// control characters are rejected. Content is stored exactly as given apart
/// from trailing whitespace.
pub fn multi_line(
    field: &'static str,
    value: &str,
    min: usize,
    max: usize,
) -> Result<String, DomainError> {
    let value = value.trim_end();
    let len = value.chars().count();
    if len < min || len > max {
        return Err(DomainError::validation(
            field,
            format!("must be between {min} and {max} characters"),
        ));
    }
    if value
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t' && c != '\r')
    {
        return Err(DomainError::validation(
            field,
            "contains unsupported control characters",
        ));
    }
    Ok(value.to_owned())
}

/// Normalizes and validates an email address. This is a structural check, not
/// proof of ownership; ownership is established by the identity provider or by
/// invitation acceptance.
pub fn email(value: &str) -> Result<String, DomainError> {
    let value = value.trim().to_lowercase();
    let invalid = || DomainError::validation("email", "must be a valid email address");
    if value.len() > 254 || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalid());
    }
    let (local, domain) = value.split_once('@').ok_or_else(invalid)?;
    if local.is_empty() || local.len() > 64 || domain.contains('@') {
        return Err(invalid());
    }
    if !domain.contains('.')
        || domain.starts_with('.')
        || domain.ends_with('.')
        || domain.contains("..")
    {
        return Err(invalid());
    }
    Ok(value)
}

/// Password policy for locally managed accounts (NIST SP 800-63B style:
/// length over composition rules).
pub fn password(value: &str) -> Result<(), DomainError> {
    let len = value.chars().count();
    if len < 12 {
        return Err(DomainError::validation(
            "password",
            "must be at least 12 characters",
        ));
    }
    if len > 256 {
        return Err(DomainError::validation(
            "password",
            "must be at most 256 characters",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_trims_and_bounds() {
        assert_eq!(
            single_line("name", "  Atlas  ", 1, 10).expect("ok"),
            "Atlas"
        );
        assert!(single_line("name", "   ", 1, 10).is_err());
        assert!(single_line("name", "abcdefghijk", 1, 10).is_err());
        assert!(single_line("name", "a\nb", 1, 10).is_err());
        // Length counts characters, not bytes.
        assert!(single_line("name", "ééééé", 1, 5).is_ok());
    }

    #[test]
    fn multi_line_allows_newlines_but_not_other_controls() {
        assert!(multi_line("body", "line 1\nline 2\tx", 1, 100).is_ok());
        assert!(multi_line("body", "bell\u{7}", 1, 100).is_err());
        assert_eq!(multi_line("body", "", 0, 10).expect("ok"), "");
    }

    #[test]
    fn email_is_normalized() {
        assert_eq!(email("  Ada@Example.ORG ").expect("ok"), "ada@example.org");
        for bad in [
            "",
            "ada",
            "ada@",
            "@example.org",
            "a@b",
            "a@@b.org",
            "a b@c.org",
            "a@.org",
            "a@b..org",
        ] {
            assert!(email(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn password_length_policy() {
        assert!(password("short").is_err());
        assert!(password("correct horse battery").is_ok());
        assert!(password(&"x".repeat(257)).is_err());
    }
}
