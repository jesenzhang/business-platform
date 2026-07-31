//! Validated object key that prevents path traversal attacks.

use std::fmt;
use thiserror::Error;

/// Error when constructing an invalid `ObjectKey`.
#[derive(Debug, Error)]
pub enum ObjectKeyError {
    #[error("object key is empty")]
    Empty,
    #[error("object key contains path traversal sequence '..'")]
    PathTraversal,
    #[error("object key is an absolute path")]
    AbsolutePath,
    #[error("object key contains Windows drive letter")]
    WindowsDrive,
    #[error("object key contains UNC path prefix")]
    UncPath,
    #[error("object key contains empty segment")]
    EmptySegment,
    #[error("object key contains NUL character")]
    NulCharacter,
    #[error("object key exceeds maximum length of {0}")]
    TooLong(usize),
}

const MAX_KEY_LENGTH: usize = 1024;

/// A validated object storage key.
///
/// Guarantees:
/// - No `..` sequences
/// - No absolute paths (Unix `/` or Windows `C:\`)
/// - No UNC paths (`\\server\share`)
/// - No NUL characters
/// - No empty segments (`//` or trailing `/`)
/// - Maximum length enforced
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectKey(String);

impl ObjectKey {
    /// Construct a new `ObjectKey`, validating the input.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectKeyError`] if the key violates any safety invariant.
    pub fn new(key: impl Into<String>) -> Result<Self, ObjectKeyError> {
        let key = key.into();
        Self::validate(&key)?;
        Ok(Self(key))
    }

    fn validate(key: &str) -> Result<(), ObjectKeyError> {
        if key.is_empty() {
            return Err(ObjectKeyError::Empty);
        }
        if key.len() > MAX_KEY_LENGTH {
            return Err(ObjectKeyError::TooLong(MAX_KEY_LENGTH));
        }
        if key.contains('\0') {
            return Err(ObjectKeyError::NulCharacter);
        }
        // UNC path (must precede absolute-path checks since `//` also starts with `/`)
        if key.starts_with("\\\\") || key.starts_with("//") {
            return Err(ObjectKeyError::UncPath);
        }
        // Absolute path checks
        if key.starts_with('/') || key.starts_with('\\') {
            return Err(ObjectKeyError::AbsolutePath);
        }
        // Windows drive letter: C:\ or C:/
        if key.len() >= 2 && key.as_bytes()[0].is_ascii_alphabetic() && key.as_bytes()[1] == b':' {
            return Err(ObjectKeyError::WindowsDrive);
        }
        // Check segments
        for segment in key.split(['/', '\\']) {
            if segment == ".." {
                return Err(ObjectKeyError::PathTraversal);
            }
            if segment.is_empty() {
                return Err(ObjectKeyError::EmptySegment);
            }
        }
        Ok(())
    }

    /// Get the key as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ObjectKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for ObjectKey {
    type Err = ObjectKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_keys() {
        assert!(ObjectKey::new("documents/tenant-1/file.pdf").is_ok());
        assert!(ObjectKey::new("a/b/c.txt").is_ok());
        assert!(ObjectKey::new("single-file.txt").is_ok());
        assert!(ObjectKey::new("path/with-dashes/and_underscores/file.tar.gz").is_ok());
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(ObjectKey::new(""), Err(ObjectKeyError::Empty)));
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(matches!(
            ObjectKey::new("../etc/passwd"),
            Err(ObjectKeyError::PathTraversal)
        ));
        assert!(matches!(
            ObjectKey::new("foo/../bar"),
            Err(ObjectKeyError::PathTraversal)
        ));
        assert!(matches!(
            ObjectKey::new("foo/.."),
            Err(ObjectKeyError::PathTraversal)
        ));
        assert!(matches!(
            ObjectKey::new("..\\windows\\system32"),
            Err(ObjectKeyError::PathTraversal)
        ));
    }

    #[test]
    fn rejects_absolute_paths() {
        assert!(matches!(
            ObjectKey::new("/etc/passwd"),
            Err(ObjectKeyError::AbsolutePath)
        ));
        assert!(matches!(
            ObjectKey::new("\\Windows\\System32"),
            Err(ObjectKeyError::AbsolutePath)
        ));
    }

    #[test]
    fn rejects_windows_drive() {
        assert!(matches!(
            ObjectKey::new("C:\\Users\\admin"),
            Err(ObjectKeyError::WindowsDrive)
        ));
        assert!(matches!(
            ObjectKey::new("D:/data"),
            Err(ObjectKeyError::WindowsDrive)
        ));
    }

    #[test]
    fn rejects_unc_paths() {
        assert!(matches!(
            ObjectKey::new("\\\\server\\share"),
            Err(ObjectKeyError::UncPath)
        ));
        assert!(matches!(
            ObjectKey::new("//server/share"),
            Err(ObjectKeyError::UncPath)
        ));
    }

    #[test]
    fn rejects_empty_segments() {
        assert!(matches!(
            ObjectKey::new("foo//bar"),
            Err(ObjectKeyError::EmptySegment)
        ));
        assert!(matches!(
            ObjectKey::new("foo/"),
            Err(ObjectKeyError::EmptySegment)
        ));
    }

    #[test]
    fn rejects_nul() {
        assert!(matches!(
            ObjectKey::new("foo\0bar"),
            Err(ObjectKeyError::NulCharacter)
        ));
    }

    #[test]
    fn rejects_too_long() {
        let long_key = "a/".repeat(600);
        assert!(matches!(
            ObjectKey::new(long_key),
            Err(ObjectKeyError::TooLong(_))
        ));
    }
}
