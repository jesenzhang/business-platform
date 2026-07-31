use std::fmt;

use serde::Deserialize;
use thiserror::Error;
use url::Url;

use crate::Secret;

const REDACTED: &str = "***";
const SENSITIVE_QUERY_KEYS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "token",
    "access_token",
    "refresh_token",
    "api_key",
    "apikey",
    "secret",
    "client_secret",
    "signature",
    "sig",
];

/// A parsed connection URL that exposes plaintext only at an explicit client
/// construction boundary.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretUrl {
    exposed: Secret<String>,
    redacted: String,
}

#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
#[error("invalid connection URL")]
pub struct SecretUrlParseError;

impl SecretUrl {
    pub fn parse(value: &str) -> Result<Self, SecretUrlParseError> {
        let mut parsed = Url::parse(value).map_err(|_| SecretUrlParseError)?;
        let redacted = redact(&mut parsed);
        Ok(Self {
            exposed: Secret::new(value.to_owned()),
            redacted,
        })
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        self.exposed.expose()
    }

    #[must_use]
    pub fn redacted(&self) -> &str {
        &self.redacted
    }
}

impl fmt::Debug for SecretUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.redacted)
    }
}

impl fmt::Display for SecretUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.redacted)
    }
}

impl<'de> Deserialize<'de> for SecretUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

fn redact(url: &mut Url) -> String {
    if url.password().is_some() {
        let _ = url.set_password(Some(REDACTED));
    } else if !url.username().is_empty() {
        let _ = url.set_username(REDACTED);
    }

    if url.query().is_some() {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in url.query_pairs() {
            let replacement = if is_sensitive_key(&key) {
                REDACTED
            } else {
                value.as_ref()
            };
            serializer.append_pair(&key, replacement);
        }
        url.set_query(Some(&serializer.finish()));
    }

    url.to_string()
}

fn is_sensitive_key(key: &str) -> bool {
    SENSITIVE_QUERY_KEYS
        .iter()
        .any(|candidate| key.eq_ignore_ascii_case(candidate))
}
