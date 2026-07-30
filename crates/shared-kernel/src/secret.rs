use std::fmt;

/// A wrapper that prevents secrets from appearing in Debug, Display, or logs.
///
/// `Secret` intentionally does NOT implement `Serialize` to prevent accidental
/// serialization into logs, API responses, or other output channels.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    /// Create a new secret value.
    #[must_use]
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Access the inner value. Call sites should be audited.
    #[must_use]
    pub fn expose(&self) -> &T {
        &self.0
    }

    /// Consume and return the inner value.
    #[must_use]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl<T> fmt::Display for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Secret<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        T::deserialize(deserializer).map(Secret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_debug_is_redacted() {
        let secret = Secret::new("super-secret-password");
        let debug_output = format!("{secret:?}");
        assert!(!debug_output.contains("super-secret-password"));
        assert!(debug_output.contains("REDACTED"));
    }

    #[test]
    fn secret_display_is_redacted() {
        let secret = Secret::new("my-api-key-12345");
        let display_output = format!("{secret}");
        assert!(!display_output.contains("my-api-key-12345"));
        assert!(display_output.contains("REDACTED"));
    }

    #[test]
    fn secret_expose_returns_value() {
        let secret = Secret::new("actual-value");
        assert_eq!(secret.expose(), &"actual-value");
    }

    #[test]
    fn secret_into_inner_returns_value() {
        let secret = Secret::new(String::from("inner"));
        assert_eq!(secret.into_inner(), "inner");
    }

    #[test]
    fn secret_equality() {
        let a = Secret::new("same");
        let b = Secret::new("same");
        let c = Secret::new("different");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
