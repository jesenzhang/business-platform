#![allow(clippy::expect_used)]

use runtime_config::SecretUrl;

const DATABASE_PASSWORD: &str = "DO_NOT_LEAK_DATABASE_PASSWORD";
const NATS_TOKEN: &str = "DO_NOT_LEAK_NATS_TOKEN";
const QUERY_SECRET: &str = "DO_NOT_LEAK_QUERY_SECRET";

#[test]
fn database_credentials_are_redacted_in_display_and_debug() {
    let value = SecretUrl::parse(&format!(
        "postgres://user:{DATABASE_PASSWORD}@db.internal:5432/platform"
    ))
    .expect("database URL should parse");

    for rendered in [
        value.to_string(),
        format!("{value:?}"),
        value.redacted().to_string(),
    ] {
        assert!(rendered.contains("db.internal"));
        assert!(!rendered.contains(DATABASE_PASSWORD));
        assert!(rendered.contains("***"));
    }
}

#[test]
fn nats_and_sensitive_query_parameters_are_redacted_case_insensitively() {
    let nats = SecretUrl::parse(&format!("nats://user:{NATS_TOKEN}@nats.internal:4222"))
        .expect("NATS URL should parse");
    let https = SecretUrl::parse(&format!(
        "https://example.com/path?ToKeN={QUERY_SECRET}&page=2"
    ))
    .expect("HTTPS URL should parse");

    for rendered in [nats.to_string(), https.to_string(), format!("{https:?}")] {
        assert!(!rendered.contains(NATS_TOKEN));
        assert!(!rendered.contains(QUERY_SECRET));
    }
    assert!(https.to_string().contains("page=2"));
}

#[test]
fn invalid_input_never_appears_in_errors_or_rendering() {
    let raw = "not a URL with DO_NOT_LEAK_QUERY_SECRET";
    let error = SecretUrl::parse(raw).expect_err("invalid URL must fail");

    assert!(!error.to_string().contains(QUERY_SECRET));
    assert_eq!(error.to_string(), "invalid connection URL");
}
