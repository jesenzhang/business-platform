# jarvis-model-provider

Standalone Rust library for model protocol work. Other agent systems can depend
on this crate without taking a Jarvis Runtime dependency.

Published package name: `jarvis-model-provider`. Rust import: `jarvis_model_provider`.
Source lives in the Jarvis workspace at `crates/model-provider`.

```toml
[dependencies]
jarvis-model-provider = "0.1"
```

```rust
use jarvis_model_provider::{
    Api, CompletionRequest, Message, ModelProvider, Models,
};

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let models = Models::new().with_api_key("openai", std::env::var("OPENAI_API_KEY")?)?;
let model = models.get("openai", "gpt-4o-mini").expect("catalog model");
let provider = models.connect(&model)?;
let mut stream = provider
    .stream(CompletionRequest::new(model, vec![Message::user("Hello")]))
    .await?;
while let Some(event) = futures::StreamExt::next(&mut stream).await {
    println!("{:?}", event?);
}
# Ok(()) }
```

`Models` is the unified entry: catalog lookup, API-key auth (explicit or
`OPENAI_API_KEY` / `ANTHROPIC_API_KEY`), and transport construction.

Standalone image generation is a separate public seam because it does not
produce a conversational `Completion`. Use an `Api::OpenAiImages` model with
`Models::connect_image` or build `OpenAiImageProvider` directly:

```rust
# async fn run() -> Result<(), Box<dyn std::error::Error>> {
use jarvis_model_provider::{
    Api, ImageGenerationRequest, ImageQuality, ImageSize, ModelSpec, ProviderId,
};
use jarvis_model_provider::providers::OpenAiImageProvider;

let provider_id = ProviderId::new("openai")?;
let model = ModelSpec::custom("gpt-image-2", provider_id.clone(), Api::OpenAiImages);
let provider = OpenAiImageProvider::new(provider_id, std::env::var("OPENAI_API_KEY")?)?;
let response = provider
    .generate(
        ImageGenerationRequest::new(model, "a red kite over a blue lake")
            .with_size(ImageSize::Landscape)
            .with_quality(ImageQuality::High),
    )
    .await?;
assert!(!response.data.is_empty());
# Ok(()) }
```

The standalone transport targets the OpenAI Images API
`/images/generations`. It supports prompt generation, count, size, quality,
background, output format, response format, base64 image results, URL results,
revised prompts, and image usage when returned by the provider. Image edits,
variations, Responses API image-generation tools, and streaming image events
are separate capabilities and are not silently implied by this seam. See the
[OpenAI images guide](https://developers.openai.com/api/docs/guides/images-vision)
and [Images API reference](https://developers.openai.com/api/reference/resources/images)
for the provider-side contract.

Provider profiles are immutable snapshots of endpoint policy, authentication
requirements, a current `ModelCatalog`, and an optional remote model source.
Publish a complete profile synchronously, read its last-known snapshot, or
refresh its catalog without exposing a partially-updated catalog:

```rust
# async fn run() -> Result<(), Box<dyn std::error::Error>> {
use jarvis_model_provider::{
    Api, AuthRequirement, ModelCatalog, Models, ProviderId, ProviderProfile,
    RemoteModelSource,
};

let provider = ProviderId::new("openai-compatible")?;
let profile = ProviderProfile::new(
    provider.clone(),
    Api::OpenAiCompletions,
    ModelCatalog::default(),
)
.with_auth(AuthRequirement::Optional)
.with_remote_model_source(RemoteModelSource::models());
let models = Models::new().with_profile(profile)?;
let last_known = models.profile("openai-compatible");
let _ = models.refresh("openai-compatible", None).await?;
assert!(last_known.is_some());
# Ok(()) }
```

Profiles never contain credentials. `AuthRequirement::None` performs no
credential lookup and rejects request-level `Authorization` and `x-api-key`
headers before dispatch. `Optional` uses a stored or environment credential
when present, while `Required` fails before network I/O when no credential is
available. The same profile contract is enforced for Chat Completions,
Responses, and Anthropic connections.

Credential lifecycle is separate from profile and catalog data. The existing
`Credential`/`CredentialStore` API remains the API-key path. A
`MemoryCredentialStore` can also hold an `OAuthCredential` with an access token,
refresh token, and expiry. `Models::with_credential_refresher` installs an
application-owned async refresh strategy; concurrent requests for one expired
credential are coalesced and a refreshed credential is published atomically.
`Models::set_credential`, `clear_credential`, and `revoke_credential` provide
local re-authentication and removal boundaries. A clear or revoke blocks
environment fallback until a new credential is set. Refresh failure leaves the
old credential untouched and falls back to the provider environment API key
when one is configured and the credential has not been revoked. Custom
`CredentialStore` implementations that support OAuth must override `resolve`
with an atomic refresh/publish operation; the compatibility default refuses
expired OAuth credentials rather than risking stale publication. Stores that
implement clear/revoke should also report the revocation through
`is_revoked`.

OAuth login, browser interaction, and provider-specific token exchange remain
application responsibilities. The crate supports externally-issued Anthropic
and OpenAI Workload Identity Federation bearer credentials on the existing
Messages, Chat Completions, and Responses APIs, and emits the documented
`Authorization` header. The application refresh callback may exchange a new
workload identity token even when the provider does not issue a refresh token.
Interactive user OAuth login is explicitly excluded; API-key authentication
remains supported for every admitted protocol. See the
[OpenAI API authentication documentation](https://platform.openai.com/docs/api-reference/authentication)
and [OpenAI workload identity token exchange reference](https://developers.openai.com/api/reference/workload-identity-federation)
and [Claude API authentication overview](https://platform.claude.com/docs/en/api/overview)
for the provider-side contracts.

`refresh` is abort-aware when passed `Some(AbortSignal)` and publishes only
after the fetched catalog validates against the profile's API and provider.
`refresh_with_outcome` exposes `Published`, `SkippedNoSource`, `Aborted`,
`Superseded`, and `Failed` states. Static profiles without a remote source are
deterministic no-ops. A network, parse, validation, store, or abort failure
leaves the previous snapshot published; a newer-started refresh supersedes an
older one.

When a registered profile matches `ModelSpec.provider`, the generic `get` plus
`connect` path uses that profile as the connection authority. The supplied
model's model-level metadata and compatibility settings remain in force, while
the profile supplies the API, base URL, endpoint policy, authentication
requirement, credential resolution, and request-header enforcement. An
explicit `connect_with_api` override must match the profile API; a conflicting
override is rejected. With no matching profile, builtin/static catalog
connection behavior is unchanged.

The optional async `ModelsStore` boundary retains only non-secret catalog facts
(`ModelSpec` values, freshness, optional HTTP validators, source identity, and
a persisted monotonic generation). Endpoint URLs, endpoint policy, auth
requirements, and credentials are never restored from storage. Restoring a
stored generation seeds the next refresh with a strictly newer generation;
in-process newer-started-wins publication remains independent of that
persisted revision. An in-memory store can be shared for offline restore:

```rust
# async fn run() -> Result<(), Box<dyn std::error::Error>> {
use std::sync::Arc;
use jarvis_model_provider::{
    Api, AuthRequirement, InMemoryModelsStore, ModelCatalog, Models, ProviderProfile,
    RemoteModelSource,
};

let store = Arc::new(InMemoryModelsStore::new());
let profile = ProviderProfile::new(
    "openai-compatible".parse()?,
    Api::OpenAiCompletions,
    ModelCatalog::default(),
)
.with_auth(AuthRequirement::None)
.with_remote_model_source(RemoteModelSource::models());
let online = Models::new()
    .with_models_store(store.clone())
    .with_profile(profile.clone())?;
let _ = online.refresh("openai-compatible", None).await?;

let offline = Models::new()
    .with_models_store(store)
    .with_profile(profile)?;
let _ = offline.restore("openai-compatible").await?; // no network call
# Ok(()) }
```

Discovery uses one visible overlay. In the absence of a matching profile, an
explicitly supplied `ModelSpec` remains the static connection input;
profile/builtin records win over remote partial records; remote records may
add IDs but cannot erase curated models or copy compatibility from an
unrelated model. Newly discovered models remain capability-unknown. Use the
owned `list_snapshot`,
`list_provider_snapshot`, and `available_snapshot` APIs for profile-aware
discovery; the older borrowed catalog views are retained only for static
backwards compatibility.

Endpoint policy is shared by provider and catalog HTTP clients. Both policies
disable ambient proxies; `TrustedPrivateHttp` also rejects redirects, and
credential-bearing catalog requests reject redirects under either policy.
Application-owned filesystem persistence, browser login/token exchange, runtime
routing/retry/fallback, and transports beyond the three conversational APIs and
the standalone Images API remain out of scope.

Current transports:

- `Api::OpenAiCompletions` for OpenAI Chat Completions-compatible endpoints.
- `Api::OpenAiResponses` for the OpenAI Responses API.
- `Api::AnthropicMessages` for Anthropic Messages.
- `Api::OpenAiImages` for standalone OpenAI Images API generation at
  `/images/generations`.

Shared request features:

- streaming and non-streaming completion
- tools and `tool_choice`
- image input; image tool-result parts are retained in the neutral model and
  budget accounting, but each transport validates wire support independently
- request-side reasoning (`ReasoningConfig`)
- cooperative abort (`AbortSignal` / `RequestOptions`)
- extra HTTP headers
- usage tokens, cache/reasoning counters, and `ModelSpec::cost_for`
- bounded diagnostics, `FailurePhase`, and `Retry-After`

All completion paths use the same request preparation boundary. Callers that
need to inspect normalized semantics before dispatch can call
`prepare_request(&Api, request)` or `ModelProvider::prepare_request`; the
returned `PreparedRequest` contains the target-normalized request, budget, and
`HistoryNormalization` diagnostics. Direct providers, profile-backed
`Models::connect`, credential-backed providers, streaming, completion, and
validation share this preparation path.

Tool constraints are provider-neutral and fail before dispatch when the
selected wire protocol cannot represent them. `ToolSpec::constraint` expresses
strict JSON Schema or an explicit grammar for tool inputs;
`CompletionRequest::output_constraint` expresses JSON Schema or grammar for
final output. Grammar is represented for capability negotiation and currently
rejected by all three transports; it is never compiled by this crate. The
current matrix is intentionally conservative:

The new fields are optional on the serialized contract. Existing Rust callers
that use public struct literals must add `constraint: None` to `ToolSpec` and
`output_constraint: None` and `continuation: None` to `CompletionRequest`, and
`continuation: None` to `Completion`, and `portability` to
`ReasoningContent`; `ToolSpec::new`,
`ToolSpec::with_constraint`, `CompletionRequest::new`, and
`CompletionRequest::with_output_constraint` / `with_continuation` avoid that
literal migration.

Provider-bound Anthropic reasoning is constructed with an
`AnthropicMessagesContinuation`; its signature and redacted payload are not
fields on `ReasoningContent`.

| Protocol | Strict tool schema | JSON Schema output | Grammar |
| --- | --- | --- | --- |
| OpenAI Chat Completions | native `function.strict` | native `response_format.json_schema` | unsupported |
| OpenAI Responses | native function-tool `strict` | native `text.format` | unsupported |
| Anthropic Messages | native `tools[].strict` | native `output_config.format` | unsupported |

The protocol capability declaration is exposed through
`ModelProvider::constraint_capabilities` and
`protocol_constraint_capabilities`. Model metadata may further reject known
models whose `structured_output` capability is false for final structured
output. Tool strictness is declared separately by the provider capability
matrix. All constraint schemas are validated as JSON objects before dispatch,
bounded to 64 KiB compact JSON and 32 levels of nesting; grammar expressions
are bounded to 64 KiB. A structured
output constraint cannot be combined with forced `Required` or specific-tool
selection; `Auto`, `None`, or an omitted choice can coexist with tools.
Reasoning may coexist with a supported strict schema path, except Anthropic
manual thinking, which rejects forced `any`/specific-tool choice before
dispatch. Anthropic structured output currently requires the strict native
format; a non-strict request is rejected rather than silently strengthened.

Token budgeting is provider-neutral. `estimate_request_budget` reports bounded
input estimates for messages, tool schemas, and image parts, the explicitly
requested output reservation (or the declared model maximum for conservative
context capacity), the model context window, remaining output capacity, and a
separate reasoning sub-ledger. `TokenEstimator` allows an
application tokenizer to replace the default heuristic and publish `Exact`
precision; the default is explicitly `Bounded` and is not provider billing
usage. Missing context or output metadata remains `Unknown`.

In particular, Responses currently accepts text tool results only and rejects
an image tool-result part before dispatch; the neutral image budget still
accounts for that part so callers can make the same context decision before
selecting a compatible transport.

Usage has one normalized accounting contract: `input_tokens` is total logical
input, `total_tokens = input_tokens + output_tokens`, and optional cache
subdivisions satisfy `input_tokens = uncached + cache_read + cache_write`.
Reasoning tokens are a sub-ledger of output. `ModelSpec::cost_for` charges the
normal input rate only on uncached input; use `try_calculate_cost` when malformed
provider usage must be surfaced as `UsageError` instead of handled by the safe
compatibility helper.

Known context overflow and an explicit output reservation above the model's
declared maximum fail in `BeforeDispatch`. If output metadata is absent, the
crate does not invent a provider default or silently truncate history;
truncation, summarization, and retry decisions remain with Runtime or its
caller. Streaming and non-streaming wire usage share `Usage`; cache counters
remain separate dimensions and reasoning counters remain a sub-ledger of
output rather than being counted twice in `total_tokens`.

Conversation history is provider-neutral and can be handed from any of the
three transports to another. `normalize_history` provides a target-protocol
preview; `normalize_request_history` additionally applies the declared
model's known reasoning capability and matches the transport's pre-validation
history. The adapters apply the same normalization immediately before
validation: unsigned reasoning becomes assistant text for Anthropic.
Provider-bound reasoning is retained only when a matching typed continuation
sidecar is available; otherwise Anthropic drops it rather than pretending a
signature survived, while a cross-provider textual summary may be explicitly
downgraded. Redacted reasoning is retained only by transports with a
corresponding opaque sidecar field (Anthropic `redacted_thinking` or Responses
`encrypted_content`). Chat Completions drops redacted blocks because it has no
equivalent field. These decisions are deterministic and reported by
`HistoryNormalization`; they do not fail a request merely because history
came from another provider.

Assistant content order and tool-call IDs are retained by the normalized
history and by the Responses/Anthropic wire adapters. Chat Completions uses
its protocol-defined separate reasoning and `tool_calls` fields. If a stream
is aborted, `StreamAccumulator::partial_message()` exposes the collected
history for a subsequent request without pretending that the interrupted
stream was a completed response.

Provider-native continuation state is kept beside, not inside, normalized
messages. `ReasoningContent` contains only normalized text, a redacted marker,
and the provider-neutral `ReasoningPortability` classification; Anthropic
signatures and redacted payloads live in `AnthropicMessagesContinuation`.
`Completion.continuation` and `StreamEvent::Continuation` expose the typed
`ProviderContinuation`; OpenAI Responses state carries provider/API/model
identity, uses `previous_response_id` for retained responses, and replays
encrypted reasoning items for ephemeral responses. Continuation durability is
classified for the owning Runtime (`ProviderBound` or
`SensitiveNonDurable` here); the crate does not persist, retry, or recover it.

Auth resolution order: the configured credential store (including an
application-supplied OAuth refresh strategy), then environment API keys. The
crate never serializes access/refresh tokens into profiles or catalog stores,
and credential/debug/error paths redact token material. It does not decide
retry, routing, fallback, tool execution, or Runtime recovery.
