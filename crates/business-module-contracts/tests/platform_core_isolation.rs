const BUSINESS_MODULE_CONTRACTS_MANIFEST: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
const BUSINESS_MODULE_CONTRACTS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
const SEMANTIC_CONTRACT_MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../semantic-contract/Cargo.toml"
));

#[test]
fn platform_core_does_not_reference_fixture_business_modules() {
    for source in [
        BUSINESS_MODULE_CONTRACTS_MANIFEST,
        BUSINESS_MODULE_CONTRACTS_SOURCE,
        SEMANTIC_CONTRACT_MANIFEST,
    ] {
        assert!(!source.contains("fixture-sales"));
        assert!(!source.contains("fixture-finance"));
    }
}
