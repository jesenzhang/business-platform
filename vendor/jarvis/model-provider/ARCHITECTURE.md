# `jarvis-model-provider` Architecture

This crate is the standalone model-protocol boundary. It turns a
provider-neutral request into one provider/API wire exchange and returns
normalized messages, usage, stream events, and provider facts. It does not
own Jarvis Runtime lifecycle, retry, recovery, routing, tool execution, or
durable-effect orchestration.

## Layers

The public model is deliberately split into orthogonal layers:

```text
Provider
  ├─ credentials and endpoint identity
  ├─ Models/catalog selection
  └─ API protocol
       └─ Dialect / wire policy

Model
  └─ capabilities and compatibility metadata

PreparedRequest
  └─ normalized history, target-aware diagnostics, and token budget

Wire codec
  └─ protocol-specific request/response/SSE encoding

Normalized stream
  └─ messages, tool calls, usage, stop facts, and continuation sidecars

ProviderContinuation
  └─ typed provider/API-bound state kept outside normalized Message content
```

`ProviderId` identifies a provider family or endpoint owner. `Api` identifies
the wire protocol. A model may carry capability metadata, but capability is a
claim about what that model supports; it is not a dialect selector. Dialect
selection belongs to the protocol compatibility/profile data used by the wire
codec.

The dependency direction is one-way:

```text
jarvis-model-provider
  └─ standalone Rust types, codecs, HTTP/SSE transport, and provider facts

jarvis-adapters / Jarvis Runtime
  └─ depends on the provider crate and owns application/runtime policy
```

`jarvis-model-provider` does not depend on `jarvis-core`, `jarvis-runtime`, a
durability layer, or a Jarvis-specific effect journal. The Jarvis provider
adapter may translate provider results into Runtime contracts, but the
provider crate remains usable by an external consumer by itself.

## Request lifecycle

Every conversational request follows one semantic path:

```text
Raw CompletionRequest
        ↓
Prepare
        ↓
PreparedRequest
        ↓
Validate
        ↓
Encode
        ↓
Dispatch
        ↓
Decode
        ↓
Normalize
```

`prepare_request(target, request)` performs target-aware history
normalization, reasoning portability decisions, deterministic defaults, and
budget estimation. `PreparedRequest` exposes the normalized request, the
`HistoryNormalization` diagnostics, and the `RequestTokenBudget` without
exposing wire-only structures.

`ModelProvider::prepare_request` is the trait seam used by direct providers,
profile-backed `Models::connect` providers, and credential-backed providers.
Concrete `stream` and `complete` implementations prepare once, validate that
prepared value, and encode it. The compatibility `validate_request` method
prepares a clone and then validates it, so preview/validation observes the
same semantics as dispatch. Authentication resolution remains outside
preparation; it happens only when the request is ready to dispatch.

History transformations are deterministic and observable. If a target cannot
carry a provider-bound or redacted reasoning block, the prepared diagnostics
report downgrade/drop counts; an opaque provider state is never silently
reinterpreted as ordinary portable text.

## Provider, API, dialect, and capability policy

The protocol and dialect axes are intentionally separate. OpenAI Chat
Completions-compatible endpoints are a protocol family, not one universal
wire dialect. `OpenAiThinkingDialect` selects a structured
`ReasoningWirePolicy`, which owns the reasoning enable/disable field, effort or
budget field, and assistant-history field for that dialect. The request and
history codecs consume the same policy, so a Qwen request cannot accidentally
replay as an OpenAI `reasoning_content` request and an OpenAI request cannot
leak Qwen `thinking_budget`.

The legacy `supports_reasoning_effort` compatibility member is retained only
for existing serialized profiles/source callers that need to opt out of an
OpenAI-style effort field. It cannot turn on a foreign dialect field. New
protocol behavior must be added to the structured dialect policy, not as an
unrelated `supports_*` boolean.

Model capabilities remain separate. A model can reject reasoning, tools,
vision, or structured output even when the selected protocol/dialect knows how
to encode those fields. Capability validation happens after preparation and
before any network request.

## Usage invariant and cost

`Usage` has one normalized accounting contract:

```text
input_tokens = uncached_input_tokens
              + cache_read_tokens
              + cache_write_tokens
total_tokens = input_tokens + output_tokens
```

Cache subdivisions are optional when a provider does not report them, but
`input_tokens` always means total logical input processed. Reasoning tokens are
a sub-ledger of `output_tokens`; they are not added a second time. Providers
normalize their wire payloads to this shape before a completion is returned or
a usage stream event is accepted. Malformed totals or subdivisions produce a
protocol error rather than an invalid completion.

Cost applies the normal input rate only to uncached input and applies cache-read
and cache-write rates to their respective dimensions. `try_calculate_cost`
returns a typed `UsageError` for malformed accounting; the infallible
`calculate_cost` remains a safe compatibility helper and never underflows or
double-counts cache tokens.

## Continuation semantics

Completion-generated continuation state does not enter normalized `Message`
content. In particular, `response_id`, `previous_response_id`, output-item
identity, Responses encrypted reasoning blobs, Anthropic signatures, and
Anthropic redacted payloads are never emitted as normalized response message
fields. `ReasoningContent` contains normalized text, a redacted marker, the
provider-neutral `ReasoningPortability` classification, and — only for
provider-bound blocks — a non-secret `ContinuationRef`. It never contains
signatures, encrypted payloads, or raw redacted-thinking bodies. A
`ProviderBound` marker requires a matching typed sidecar; losing that sidecar
cannot turn the block back into ordinary replayable history.

`Completion.continuation` and `StreamEvent::Continuation` carry typed
`ProviderContinuation` sidecars. The current implementation admits Anthropic
Messages and OpenAI Responses continuations, each with provider, API, and
model identity.

Conversation Truth is not Provider Native Continuation State: the normalized
history is the portable record of what was said, while the sidecar holds the
provider-native facts needed to keep talking to one specific provider. The two
are deliberately kept apart, and server coverage is deliberately not the same
thing as stateless manual replay:

```text
                        ProviderContinuation

                 ┌─────────────┴─────────────┐
                 │                           │
             Stateful                    Stateless
                 │                           │
        ServerStoredPrefix             ManualReplay
                 │                           │
        ContinuationScope        anchored replay segments
                 │                           │
         trim covered prefix       walk full history
                 │                  substitute exact assistant
                 │                  output projections
                 ▼                           ▼
              suffix                  complete wire history
```

- **Stateful** (`ProviderBound` durability): the provider retains a verified
  conversation prefix identified by `previous_response_id`.
  `AnthropicMessagesContinuation` and stateful
  `OpenAiResponsesContinuation` keep that coverage in a `ContinuationScope`
  (covered count + prefix digest + boundary identity). Request encoding
  validates the covered prefix and transmits only the uncovered suffix;
  edited, forked, or shortened covered history fails before dispatch.

- **Stateless** (`SensitiveNonDurable` durability): store=false / ephemeral
  Responses retain nothing. The provider retains no conversation prefix, so
  complete required historical user/tool input always remains in the wire
  input; there is no blanket prefix trimming. Provider-native assistant
  outputs are substituted through anchored `OpenAiResponsesReplaySegment`s:
  each segment binds an ordered list of typed `OpenAiResponsesReplayItem`s
  (reasoning items — encrypted or portable —, function calls, and assistant
  messages) to the exact sequence-sensitive `HistoryMessageId` of the
  normalized assistant message it reproduces.

### Sequence-sensitive history identity

`HistoryMessageId` is a domain-separated chain digest over normalized
content, not a content-only digest:

```text
id(n) = SHA256("jarvis-model-provider/history-message/v1",
               id(n-1), canonical(message(n)))
```

The chain runs over non-System conversation messages in order (System
messages are request-level instructions and stay outside the conversation
coverage boundary). Consequences:

1. **Identity is derived from the exact normalized history prefix, not from
   message content alone.** Identical messages at different history points
   receive different identities.
2. **Any earlier edit, insert, delete, or reorder invalidates every
   downstream identity** — including anchors bound to untouched messages,
   because their prefix changed.
3. The identity is deterministic, non-secret, safe to log and persist,
   independent of provider-native encrypted payloads, and stable across
   serialization/recovery.

`history_message_ids(messages)` is the single authoritative source of
history identity. Replay validation, wire encoding, and completion decoding
all derive anchors through it; new assistant outputs anchor through their
chain position in `history + [assistant]`, computed from the exact prepared
request history on both streaming and non-streaming paths. Duplicate
anchors are impossible for distinct occurrences: two identical normalized
assistants bind two different segments, each resolving by identity rather
than collection position.

### Complete output projection

`ReplaySegment.items` is the complete ordered projection of one provider
output: if the model emitted reasoning, then a function call, then more
reasoning, then a message, the segment holds exactly those four items in
that order. Portable reasoning summaries produce replay items just like
encrypted ones; only the payload differs (`encrypted_content` present with
a continuation reference versus absent without one). Stateless manual replay
therefore never silently drops a portable summary that the anchored
assistant still carries.

Validation enforces projection consistency: a deterministic provider-neutral
projection of each segment's items is compared against its anchored
normalized assistant message — text, reasoning summary text with redaction
state, tool-call id/name/canonical arguments, part count, and order.
Provider-native-only fields (`item_id`, `encrypted_content`) are excluded
from projection equality but remain validated separately. A segment that
does not reproduce its anchored assistant fails closed before dispatch.

### Fail-closed protocol encoding

Protocol encoding fails closed: malformed continuation state — missing
segments for provider-bound reasoning, stale anchors after edit/insert/
delete/reorder, duplicate or foreign segments, deserialized state that
bypasses constructor validation — produces typed
`ProviderErrorKind::InvalidRequest` errors at `FailurePhase::BeforeDispatch`.
No ordinary protocol path requires panic-based caller discipline; callers
never need to run validation first to avoid a panic (running validation
first remains recommended for earlier diagnostics).

Stateful server coverage (previous_response_id) remains distinct from
stateless manual replay: stateful continuations keep `ContinuationScope`
coverage and no segments, stateless continuations keep segments and claim
no server coverage. All stateless Responses continuations are conservatively
classified `SensitiveNonDurable`; correctness outranks persistence
optimimization even when a continuation happens to carry only portable
reasoning. Streaming and non-streaming decoding produce equivalent
segmentation; both paths anchor only after the final normalized assistant
message is known, so an interrupted stream can never mint a valid segment.

Validation requires the continuation provider, API, and model to match the
target request. Wrong-protocol or wrong-provider reuse fails before dispatch.
The durability classification is information for the owning runtime; this
crate does not decide whether state is persisted, retried, queued, or dropped.

Qwen reasoning replay is currently process-local adapter state. Capacity
exhaustion is an explicit terminal error, never silent oldest-entry eviction;
restart/cross-process recovery therefore fails closed until a higher layer
provides an explicit durable design.

## Retry and recovery ownership

The provider crate returns normalized facts, typed errors, partial stream
state, usage diagnostics, and continuation durability metadata. It does not
retry requests, choose a fallback provider, route between endpoints, execute
tools, or recover a durable turn. Jarvis Runtime and its adapters own retry,
backpressure, routing/fallback, durable persistence, effect identity, replay,
and reconciliation policy.

This boundary is intentional: provider transport can report that a request
failed, a stream was interrupted, continuation state is sensitive, or replay
capacity is exhausted, while the Runtime decides what that fact means for the
application lifecycle.

## Public API freeze posture

Stable consumer-facing types are `ModelSpec`, `CompletionRequest`, `Message`,
`Completion`, `Usage`, `PreparedRequest`, `ReasoningPortability`,
`ProviderContinuation`, and the provider error/failure-phase contracts. Wire
JSON structs and SSE parser state remain private to protocol modules. New
continuation and dialect enums are `non_exhaustive` so adding another protocol
or dialect does not require every consumer to understand provider wire detail.
`CompletionRequest` and `ReasoningConfig` are also `#[non_exhaustive]`: future
optional request/reasoning fields do not become immediate source-breaking
changes for external consumers, which construct through `CompletionRequest::new`
plus the `with_*` builders (`with_tools`, `with_temperature`,
`with_max_output_tokens`, `with_top_p`, `with_tool_choice`, `with_reasoning`,
`with_retention`, `with_output_constraint`, `with_continuation`) and
`ReasoningConfig::enabled`/`disabled` plus its builders. Serialized wire
compatibility is unchanged by the Rust construction surface. The crate remains
one package; the internal module boundary is the extensibility seam.
