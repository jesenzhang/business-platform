//! Reproduction of the compiler's canonical contract-surface bytes.
//!
//! The published contract surface is hashed inside the pure compiler
//! (`business_application_compiler::contract_digest`). This module rebuilds the
//! *same* JSON value so the surface can be shipped as a standalone artifact
//! file: the bytes written to `package/contract/contract-surface.json` are then
//! byte-identical to the compiler's hash input, and any third-party tool that
//! hashes that single file reproduces `contract_digest` without running this
//! code.
//!
//! The field list below must stay in lockstep with the private
//! `CanonicalContractSurface` inside `contract_digest`. Exposing that struct as
//! a compiler seam would mean changing a pure crate's public API, which is out
//! of scope here; instead [`assert_surface_matches_digest`] is a drift guard —
//! if the compiler's surface ever changes shape, this fails loudly instead of
//! silently shipping a stale contract artifact.

use business_application_compiler::{
    compile, contract_digest, BusinessApplicationCompilerInput, BusinessApplicationPackage,
};
use serde::Serialize;

use crate::canonical::sha256_hex;
use crate::{ToolError, ToolResult};

/// Same fields, same order, same types as the compiler's private surface.
#[derive(Serialize)]
struct CanonicalContractSurface<'a> {
    module_id: &'a business_module_contracts::BusinessModuleId,
    data_classification: &'a [business_module_contracts::DataClassification],
    published_commands: &'a [business_module_contracts::ContractDescriptor],
    published_queries: &'a [business_module_contracts::ContractDescriptor],
    published_events: &'a [business_module_contracts::ContractDescriptor],
    resource_kinds: &'a [business_module_contracts::ResourceKindDescriptor],
    semantic_contributions: &'a [business_module_contracts::SemanticContributionDescriptor],
    agent_tool_contributions: &'a [business_module_contracts::ContributionDescriptor],
    extension_points: &'a [business_module_contracts::PublishedExtensionPoint],
}

/// Canonical JSON bytes of one package's published contract surface.
///
/// `package` must already be normalized, i.e. it must be a package taken out of
/// a [`compile`] result.
pub fn contract_surface_bytes(package: &BusinessApplicationPackage) -> ToolResult<Vec<u8>> {
    let manifest = &package.manifest;
    let bytes = serde_json::to_vec(&CanonicalContractSurface {
        module_id: &manifest.module_id,
        data_classification: &manifest.data_classification,
        published_commands: &manifest.published_commands,
        published_queries: &manifest.published_queries,
        published_events: &manifest.published_events,
        resource_kinds: &manifest.resource_kinds,
        semantic_contributions: &manifest.semantic_contributions,
        agent_tool_contributions: &manifest.agent_tool_contributions,
        extension_points: &package.extension_points,
    })?;
    Ok(bytes)
}

/// Drift guard: the shipped surface bytes must hash to the compiler digest.
pub fn assert_surface_matches_digest(package: &BusinessApplicationPackage) -> ToolResult<Vec<u8>> {
    let bytes = contract_surface_bytes(package)?;
    let expected = contract_digest(package)?;
    let actual = sha256_hex(&bytes);
    if actual != expected.to_string() {
        return Err(ToolError::SurfaceDrift {
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(bytes)
}

/// Compiles a single package on its own, as a release candidate would be
/// compiled by a host that only sees this module.
pub fn compile_single(
    package: BusinessApplicationPackage,
    platform_version: &str,
) -> ToolResult<business_application_compiler::CompiledBusinessApplicationManifest> {
    let input = BusinessApplicationCompilerInput {
        platform_version: platform_version.to_owned(),
        packages: vec![package],
        ..BusinessApplicationCompilerInput::default()
    };
    Ok(compile(input)?)
}
