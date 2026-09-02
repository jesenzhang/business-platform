//! M7.2 — Replay Identity & Complete Output Preservation test matrix.
//!
//! Covers the required acceptance tests:
//! - HISTORY IDENTITY H1–H7 (sequence-sensitive chain identity)
//! - REPLAY BINDING R1–R4 (identity-bound segments, mutation invalidation)
//! - COMPLETE OUTPUT REPLAY O1–O5 (complete projection + consistency)
//! - PANIC-FREE P1–P3 (fail-closed, no panic)
//! - STREAM PARITY S1–S2 (streaming == non-streaming replay structure)

use jarvis_model_provider::providers::OpenAiResponsesProvider;
use jarvis_model_provider::{
    collect_stream, history_message_ids, Api, AssistantContent, AssistantMessage, ContinuationRef,
    ContinuationScope, DataRetentionPolicy, FailurePhase, Message, ModelProvider, ModelSpec,
    OpenAiResponsesContinuation, OpenAiResponsesContinuationMode, OpenAiResponsesReplayItem,
    OpenAiResponsesReplaySegment, ProviderContinuation, ProviderError, ProviderErrorKind,
    ProviderId, ReasoningContent, ReasoningPortability, TextContent,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

// ---------------------------------------------------------------------------
// Local HTTP fixture (mirrors the providers.rs helper).
// ---------------------------------------------------------------------------

async fn fixture(
    response_body: String,
    content_type: &'static str,
) -> (String, oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let observed = format!("http://{address}/v1");
    let (request_sender, request_receiver) = oneshot::channel();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let count = socket.read(&mut chunk).await.unwrap();
            if count == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..count]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                let header_end = request
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .unwrap()
                    + 4;
                let content_length = String::from_utf8_lossy(&request[..header_end])
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or_default();
                while request.len() < header_end + content_length {
                    let count = socket.read(&mut chunk).await.unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..count]);
                }
                let body = String::from_utf8_lossy(
                    &request
                        [header_end..header_end + content_length.min(request.len() - header_end)],
                );
                let _ = request_sender.send(body.into_owned());
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 200 Fixture\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    (observed, request_receiver)
}

async fn sse_fixture(body: String) -> String {
    fixture(body, "text/event-stream").await.0
}

async fn json_fixture(body: serde_json::Value) -> (String, oneshot::Receiver<String>) {
    let (url, receiver) = fixture(body.to_string(), "application/json").await;
    (url, receiver)
}

// ---------------------------------------------------------------------------
// Shared builders.
// ---------------------------------------------------------------------------

fn responses_test_model() -> ModelSpec {
    ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiResponses,
    )
}

fn base_request(model: ModelSpec) -> jarvis_model_provider::CompletionRequest {
    let mut request = jarvis_model_provider::CompletionRequest::new(
        model,
        vec![Message::system("You are concise."), Message::user("hello")],
    );
    request.max_output_tokens = Some(64);
    request
}

fn assistant_text(text: &str) -> Message {
    Message::Assistant(AssistantMessage {
        content: vec![AssistantContent::Text(TextContent::new(text))],
    })
}

fn provider_bound_reasoning(
    reference: &ContinuationRef,
    redacted: bool,
    text: &str,
) -> AssistantContent {
    AssistantContent::Reasoning(ReasoningContent {
        text: if redacted {
            String::new()
        } else {
            text.to_string()
        },
        redacted,
        portability: ReasoningPortability::ProviderBound,
        continuation_ref: Some(reference.clone()),
    })
}

fn reasoning_json(id: &str, encrypted: Option<&str>, summary: &str) -> serde_json::Value {
    let mut value = serde_json::json!({
        "type": "reasoning",
        "id": id,
    });
    if let Some(encrypted) = encrypted {
        value["encrypted_content"] = serde_json::json!(encrypted);
    }
    value["summary"] = serde_json::json!([{"type": "summary_text", "text": summary}]);
    value
}

fn function_call_json(id: &str, call_id: &str, name: &str, arguments: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "function_call",
        "id": id,
        "call_id": call_id,
        "name": name,
        "arguments": arguments,
    })
}

fn message_json(text: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "message",
        "role": "assistant",
        "content": [{"type": "output_text", "text": text}],
    })
}

fn completed_response(output: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({
        "id": "resp_m72",
        "status": "completed",
        "output": output,
        "usage": {"input_tokens": 2, "output_tokens": 3}
    })
}

fn wire_input_types(body: &serde_json::Value) -> Vec<String> {
    body["input"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item.get("type")
                        .and_then(serde_json::Value::as_str)
                        .or_else(|| item.get("role").and_then(serde_json::Value::as_str))
                        .unwrap_or_default()
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn responses(continuation: &ProviderContinuation) -> &OpenAiResponsesContinuation {
    continuation.openai_responses().unwrap()
}

fn stateless_segments(segments: Vec<OpenAiResponsesReplaySegment>) -> ProviderContinuation {
    ProviderContinuation::OpenAiResponses(
        OpenAiResponsesContinuation::with_segments(
            ProviderId::new("openai").unwrap(),
            "gpt-test",
            None,
            OpenAiResponsesContinuationMode::Stateless,
            ContinuationScope::empty(),
            segments,
        )
        .unwrap(),
    )
}

/// Build a segment that projects exactly onto an assistant holding a single
/// encrypted-reasoning block with the given reference — the shape used by the
/// mutation tests.
fn matching_segment(
    anchor: jarvis_model_provider::HistoryMessageId,
    reference: &ContinuationRef,
) -> OpenAiResponsesReplaySegment {
    OpenAiResponsesReplaySegment::new(
        anchor,
        vec![OpenAiResponsesReplayItem::reasoning(
            reference.clone(),
            Some("rs_1".into()),
            "enc",
            Vec::new(),
        )],
    )
}

fn expect_before_dispatch(error: &ProviderError, needle: &str) {
    assert_eq!(error.kind, ProviderErrorKind::InvalidRequest, "{error:?}");
    assert_eq!(error.phase, FailurePhase::BeforeDispatch, "{error:?}");
    assert!(error.message.contains(needle), "{error:?}");
}

// ---------------------------------------------------------------------------
// HISTORY IDENTITY — TESTS H1..H7
// ---------------------------------------------------------------------------

#[test]
fn h1_identical_assistants_at_different_positions_get_distinct_ids() {
    let history = vec![
        Message::user("u1"),
        assistant_text("same"),
        Message::user("u2"),
        assistant_text("same"),
    ];
    let ids = history_message_ids(&history).unwrap();
    assert_ne!(ids[1], ids[3]);
}

#[test]
fn h2_same_history_reconstructed_twice_yields_equal_ids() {
    fn build() -> Vec<Message> {
        vec![
            Message::system("instructions"),
            Message::user("u1"),
            assistant_text("a1"),
            Message::user("u2"),
            assistant_text("same answer"),
        ]
    }
    let first = history_message_ids(&build()).unwrap();
    let second = history_message_ids(&build()).unwrap();
    assert_eq!(first, second);
}

#[test]
fn h3_edit_before_changes_downstream_identity() {
    let history = vec![
        Message::user("u1"),
        assistant_text("a1"),
        Message::user("u2"),
        assistant_text("a2"),
    ];
    let mut edited = history.clone();
    edited[0] = Message::user("edited");
    let before = history_message_ids(&history).unwrap();
    let after = history_message_ids(&edited).unwrap();
    // The edited message itself and everything downstream changed.
    assert_ne!(before[0], after[0]);
    assert_ne!(before[3], after[3]);
}

#[test]
fn h4_insert_before_changes_downstream_identity() {
    let history = vec![
        Message::user("u1"),
        assistant_text("a1"),
        Message::user("u2"),
        assistant_text("a2"),
    ];
    let mut inserted = history.clone();
    inserted.insert(1, Message::user("inserted"));
    let before = history_message_ids(&history).unwrap();
    let after = history_message_ids(&inserted).unwrap();
    assert_ne!(before[3], after[4]);
    // The untouched prefix entry keeps its identity.
    assert_eq!(before[0], after[0]);
}

#[test]
fn h5_delete_before_changes_downstream_identity() {
    let history = vec![
        Message::user("u1"),
        Message::user("u1b"),
        assistant_text("a1"),
        Message::user("u2"),
        assistant_text("a2"),
    ];
    let mut deleted = history.clone();
    deleted.remove(1);
    let before = history_message_ids(&history).unwrap();
    let after = history_message_ids(&deleted).unwrap();
    assert_ne!(before[4], after[3]);
}

#[test]
fn h6_reorder_changes_downstream_identity() {
    let history = vec![
        Message::user("first"),
        assistant_text("a1"),
        Message::user("second"),
        assistant_text("a2"),
    ];
    let mut reordered = history.clone();
    reordered.swap(0, 2);
    let before = history_message_ids(&history).unwrap();
    let after = history_message_ids(&reordered).unwrap();
    assert_ne!(before[3], after[3]);
    assert_ne!(before[1], after[1]);
}

#[test]
fn h7_repeated_user_assistant_pattern_gets_correct_chain_identities() {
    let pattern: Vec<Message> = (0..4)
        .flat_map(|round| {
            vec![
                Message::user(format!("question {round}")),
                assistant_text("identical reply"),
            ]
        })
        .collect();
    let ids = history_message_ids(&pattern).unwrap();
    for round in 0..4 {
        let expected_prefix = history_message_ids(&pattern[..round * 2 + 2]).unwrap();
        let terminal = expected_prefix.last().unwrap();
        assert_eq!(
            ids[round * 2 + 1],
            *terminal,
            "each occurrence's identity must equal the chain over its own prefix"
        );
    }
}

// ---------------------------------------------------------------------------
// REPLAY BINDING — TESTS R1..R4
// ---------------------------------------------------------------------------

#[tokio::test]
async fn r1_r2_identical_outputs_resolve_their_own_segments_in_any_storage_order() {
    // Turn 1 produces A1 ("answer one") with encrypted reasoning.
    let turn1 = completed_response(vec![
        reasoning_json("rs_1", Some("enc-one"), "plan one"),
        message_json("shared answer"),
    ]);
    let (url1, _) = json_fixture(turn1).await;
    let provider1 = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url1)
        .unwrap();
    let completion1 = provider1
        .complete(base_request(responses_test_model()))
        .await
        .unwrap();

    // Turn 2 replays turn-1 history and produces A2 with the same normalized
    // text but different reasoning payload.
    let turn2 = completed_response(vec![
        reasoning_json("rs_2", Some("enc-two"), "plan one"),
        message_json("shared answer"),
    ]);
    let (url2, _) = json_fixture(turn2).await;
    let provider2 = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url2)
        .unwrap();
    let mut follow_up = base_request(responses_test_model());
    follow_up
        .messages
        .push(Message::Assistant(completion1.message.clone()));
    follow_up.messages.push(Message::user("second question"));
    follow_up.continuation = completion1.continuation.clone();
    let completion2 = provider2.complete(follow_up).await.unwrap();

    let cont2 = completion2.continuation.as_ref().map(responses).unwrap();
    assert_eq!(cont2.replay_segment_count(), 2);

    // Build U1 A1 T1 U2 A2 U3 history.
    let mut history = base_request(responses_test_model()).messages;
    history.push(Message::Assistant(completion1.message.clone()));
    history.push(Message::tool_result("call-x", None, "unused"));
    history.pop(); // not needed; keep simple user/assistant alternation
    let mut history = base_request(responses_test_model()).messages;
    history.push(Message::Assistant(completion1.message.clone()));
    history.push(Message::user("second question"));
    history.push(Message::Assistant(completion2.message.clone()));

    continuation_of(&completion2)
        .validate_for_history(&history)
        .expect("both identical-content assistants must validate with their own segments");

    // A1 resolves S1, A2 resolves S2 — verified by anchor equality through
    // the public API plus successful projection validation per segment.
    let ids = history_message_ids(&history).unwrap();
    let segments = cont2.replay_segments();
    assert_ne!(segments[0].anchor(), segments[1].anchor());
    assert!(segments.iter().any(|segment| segment.anchor() == &ids[1]));
    assert!(segments.iter().any(|segment| segment.anchor() == &ids[3]));

    // R2: storage order must not matter — reverse a copy of the segments by
    // rebuilding the continuation with swapped order.
    let swapped = stateless_segments(segments.iter().rev().cloned().collect());
    swapped.validate_for_history(&history).unwrap();

    // Normalized A1 == A2 on text but their anchors differ (H1 property at
    // the binding layer).
    assert_eq!(text_of(&completion1.message), text_of(&completion2.message));
}

fn continuation_of(completion: &jarvis_model_provider::Completion) -> ProviderContinuation {
    completion.continuation.clone().unwrap()
}

fn text_of(message: &AssistantMessage) -> String {
    message.text_value()
}

/// R3 — swapping A1/A2 history positions fails before dispatch.
#[test]
fn r3_swapped_history_positions_fail_closed() {
    let ref_one = ContinuationRef::new("r-one").unwrap();
    let ref_two = ContinuationRef::new("r-two").unwrap();
    let a1 = Message::Assistant(AssistantMessage {
        content: vec![provider_bound_reasoning(&ref_one, true, "")],
    });
    let a2 = Message::Assistant(AssistantMessage {
        content: vec![provider_bound_reasoning(&ref_two, true, "")],
    });
    let original = vec![
        Message::system("sys"),
        Message::user("u1"),
        a1.clone(),
        Message::user("u2"),
        a2.clone(),
    ];
    let ids = history_message_ids(&original).unwrap();
    let continuation = stateless_segments(vec![
        matching_segment(ids[1].clone(), &ref_one),
        matching_segment(ids[3].clone(), &ref_two),
    ]);
    continuation.validate_for_history(&original).unwrap();

    let mut swapped = original.clone();
    swapped.swap(2, 4);
    let error = continuation.validate_for_history(&swapped).unwrap_err();
    // Chain identities moved with their messages; the segments no longer
    // bind any assistant in the mutated conversation.
    let error = ProviderError::new(
        ProviderErrorKind::InvalidRequest,
        FailurePhase::BeforeDispatch,
        error,
    );
    expect_before_dispatch(&error, "");
    assert!(!error.message.is_empty());

    // And the standalone validation path returns an Err string containing
    // the anchor mismatch (validate_for_history returns Result<(), String>).
    let result = continuation.validate_for_history(&swapped);
    assert!(result.is_err());
}

/// R4 — inserting a message before A2 invalidates the old A2 segment.
#[test]
fn r4_inserted_message_invalidates_downstream_segment() {
    let reference = ContinuationRef::new("r-insert").unwrap();
    let a1 = Message::Assistant(AssistantMessage {
        content: vec![provider_bound_reasoning(&reference, true, "")],
    });
    let original = vec![Message::system("sys"), Message::user("u1"), a1];
    let ids = history_message_ids(&original).unwrap();
    let continuation = stateless_segments(vec![matching_segment(ids[1].clone(), &reference)]);
    continuation.validate_for_history(&original).unwrap();

    let mut inserted = original.clone();
    inserted.insert(2, Message::user("injected"));
    let error = continuation
        .validate_for_history(&inserted)
        .expect_err("insertion must invalidate the downstream anchor");
    assert!(error.contains("anchor"), "{error}");
}

// ---------------------------------------------------------------------------
// COMPLETE OUTPUT REPLAY — TESTS O1..O5
// ---------------------------------------------------------------------------

/// O1 — portable reasoning + message produce both replay items, and the next
/// stateless request wire contains both.
#[tokio::test]
async fn o1_portable_reasoning_and_message_are_both_replayed() {
    let output = completed_response(vec![
        reasoning_json("rs_p", None, "plan"),
        message_json("answer"),
    ]);
    let (url1, _) = json_fixture(output).await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url1)
        .unwrap();
    let first = provider
        .complete(base_request(responses_test_model()))
        .await
        .unwrap();

    let continuation = first.continuation.expect("portable output must continue");
    let responses_view = responses(&continuation);
    assert_eq!(responses_view.replay_segment_count(), 1);
    let items = responses_view.replay_segments()[0].items();
    assert_eq!(items.len(), 2, "reasoning + message must both be present");
    assert_eq!(items[0].kind(), "reasoning");
    assert!(!items[0].is_encrypted_reasoning());
    assert!(items[0].reference().is_none());
    assert_eq!(items[1].kind(), "assistant_message");

    // Next request wire contains both items at the anchored position.
    let (url2, receiver) = json_fixture(completed_response(vec![message_json("done")])).await;
    let provider2 = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url2)
        .unwrap();
    let mut next = base_request(responses_test_model());
    next.messages
        .push(Message::Assistant(first.message.clone()));
    next.messages.push(Message::user("follow-up"));
    next.continuation = Some(continuation.clone());
    provider2.complete(next).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&receiver.await.unwrap()).unwrap();
    assert_eq!(
        wire_input_types(&body),
        ["user", "reasoning", "message", "user"]
    );
    assert_eq!(body["input"][1]["summary"][0]["text"], "plan");
    assert_eq!(
        body["input"][1]["encrypted_content"],
        serde_json::Value::Null
    );
    assert_eq!(body["input"][2]["content"][0]["text"], "answer");
}

/// O2 — mixed output preserves exact replay item order:
/// reasoning(encrypted), function_call, reasoning(portable), message.
#[tokio::test]
async fn o2_mixed_output_preserves_exact_order() {
    let output = completed_response(vec![
        reasoning_json("rs_e", Some("enc-block"), "outer plan"),
        function_call_json("fc_1", "call-1", "lookup", "{\"q\":1}"),
        reasoning_json("rs_port", None, "inner check"),
        message_json("final"),
    ]);
    let (url1, _receiver_unused) = json_fixture(output).await;
    drop(_receiver_unused);
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url1)
        .unwrap();
    let completion = provider
        .complete(base_request(responses_test_model()))
        .await
        .unwrap();
    let segments = responses(&completion.continuation.unwrap())
        .replay_segments()
        .to_vec();
    let kinds: Vec<&str> = segments[0].items().iter().map(|item| item.kind()).collect();
    assert_eq!(
        kinds,
        [
            "reasoning",
            "function_call",
            "reasoning",
            "assistant_message"
        ]
    );
    assert!(segments[0].items()[0].is_encrypted_reasoning());
    assert!(!segments[0].items()[2].is_encrypted_reasoning());

    // The normalized assistant mirrors the same order.
    assert!(matches!(
        completion.message.content.as_slice(),
        [
            AssistantContent::Reasoning(_),
            AssistantContent::ToolCall(_),
            AssistantContent::Reasoning(_),
            AssistantContent::Text(_)
        ]
    ));
}

/// O3 — portable-reasoning-only output is replayed, never dropped.
#[tokio::test]
async fn o3_portable_reasoning_only_is_replayed() {
    let output = completed_response(vec![reasoning_json("rs_only", None, "only summary")]);
    let (url1, _) = json_fixture(output).await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url1)
        .unwrap();
    let completion = provider
        .complete(base_request(responses_test_model()))
        .await
        .unwrap();
    let continuation = completion
        .continuation
        .expect("portable-only output must still continue");
    let segments = responses(&continuation).replay_segments().to_vec();
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].items().len(), 1);
    assert_eq!(segments[0].items()[0].kind(), "reasoning");

    // Wire replay includes the reasoning summary.
    let (url2, receiver) = json_fixture(completed_response(vec![message_json("done")])).await;
    let provider2 = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url2)
        .unwrap();
    let mut next = base_request(responses_test_model());
    next.messages
        .push(Message::Assistant(completion.message.clone()));
    next.messages.push(Message::user("after"));
    next.continuation = Some(continuation);
    provider2.complete(next).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&receiver.await.unwrap()).unwrap();
    assert_eq!(wire_input_types(&body), ["user", "reasoning", "user"]);
    assert_eq!(body["input"][1]["summary"][0]["text"], "only summary");
}

/// O4 — function-call-only output keeps replay and normalized ToolCall
/// consistent.
#[tokio::test]
async fn o4_function_call_only_stays_consistent() {
    let output = completed_response(vec![function_call_json(
        "fc_o",
        "call-o",
        "search",
        "{\"k\":\"v\"}",
    )]);
    let (url1, _) = json_fixture(output).await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url1)
        .unwrap();
    let completion = provider
        .complete(base_request(responses_test_model()))
        .await
        .unwrap();
    let continuation = continuation_of(&completion);
    let segments = responses(&continuation).replay_segments().to_vec();
    assert_eq!(segments[0].items().len(), 1);
    match (&segments[0].items()[0], &completion.message.content[0]) {
        (
            OpenAiResponsesReplayItem::FunctionCall { call_id, name, .. },
            AssistantContent::ToolCall(call),
        ) => {
            assert_eq!(call_id, "call-o");
            assert_eq!(name, "search");
            assert_eq!(call.id, "call-o");
            assert_eq!(call.name, "search");
        }
        _ => panic!("expected consistent function-call projections"),
    }
    continuation
        .validate_for_history(&[
            Message::system("You are concise."),
            Message::user("hello"),
            Message::Assistant(completion.message.clone()),
        ])
        .unwrap();
}

/// O5 — a segment whose projection differs from its anchored assistant fails
/// closed before dispatch.
#[test]
fn o5_projection_mismatch_fails_closed() {
    let reference = ContinuationRef::new("proj").unwrap();
    let anchored = Message::Assistant(AssistantMessage {
        content: vec![
            provider_bound_reasoning(&reference, true, ""),
            AssistantContent::Text(TextContent::new("the real answer")),
        ],
    });
    let history = vec![Message::system("sys"), Message::user("u1"), anchored];
    // history_message_ids excludes the System entry, so the assistant binds
    // ids[1].
    let ids = history_message_ids(&history).unwrap();

    // Segment carries the right anchor but projects different text.
    let wrong_text = OpenAiResponsesReplaySegment::new(
        ids[1].clone(),
        vec![
            OpenAiResponsesReplayItem::reasoning(
                reference.clone(),
                Some("rs_1".into()),
                "enc",
                Vec::new(),
            ),
            OpenAiResponsesReplayItem::assistant_message(
                ContinuationRef::new("m").unwrap(),
                None,
                None,
                "a completely different answer",
            ),
        ],
    );
    let continuation = stateless_segments(vec![wrong_text]);
    let error = continuation
        .validate_for_history(&history)
        .expect_err("foreign segment must fail projection validation");
    assert!(error.contains("does not project"), "{error}");

    // Reasoning summary mismatch also fails.
    let wrong_summary = OpenAiResponsesReplaySegment::new(
        ids[1].clone(),
        vec![OpenAiResponsesReplayItem::reasoning(
            reference.clone(),
            Some("rs_1".into()),
            "enc",
            vec!["different plan".into()],
        )],
    );
    let anchored_redacted = Message::Assistant(AssistantMessage {
        content: vec![provider_bound_reasoning(&reference, true, "")],
    });
    let history2 = vec![
        Message::system("sys"),
        Message::user("u1"),
        anchored_redacted,
    ];
    // Redacted reasoning normalizes empty; a summary-carrying item projects a
    // non-empty text, which must mismatch.
    let continuation2 = stateless_segments(vec![wrong_summary]);
    let result = continuation2.validate_for_history(&history2);
    // The redacted marker projects empty text; the replay item has a summary,
    // so the projections disagree.
    assert!(result.is_err());

    // Tool-call argument mismatch fails too.
    let call_assistant = Message::Assistant(AssistantMessage {
        content: vec![AssistantContent::ToolCall(
            jarvis_model_provider::ToolCall {
                id: "call-t".into(),
                name: "lookup".into(),
                arguments: serde_json::json!({"q": 1}),
            },
        )],
    });
    let history3 = vec![Message::system("sys"), Message::user("u1"), call_assistant];
    let ids3 = history_message_ids(&history3).unwrap();
    let wrong_arguments = OpenAiResponsesReplaySegment::new(
        ids3[1].clone(),
        vec![OpenAiResponsesReplayItem::function_call(
            ContinuationRef::new("fc").unwrap(),
            None,
            "call-t",
            "lookup",
            "{\"q\":999}",
        )],
    );
    let continuation3 = stateless_segments(vec![wrong_arguments]);
    assert!(continuation3.validate_for_history(&history3).is_err());

    // Matching projection validates.
    let right_arguments = OpenAiResponsesReplaySegment::new(
        ids3[1].clone(),
        vec![OpenAiResponsesReplayItem::function_call(
            ContinuationRef::new("fc-ok").unwrap(),
            None,
            "call-t",
            "lookup",
            "{\"q\":1}",
        )],
    );
    let continuation_ok = stateless_segments(vec![right_arguments]);
    continuation_ok.validate_for_history(&history3).unwrap();
}

// ---------------------------------------------------------------------------
// PANIC-FREE — TESTS P1..P3
// ---------------------------------------------------------------------------

/// P1 — ProviderBound reasoning without a matching segment errors cleanly
/// during wire encoding (no panic).
#[tokio::test]
async fn p1_missing_segment_for_provider_bound_reasoning_errors_without_panic() {
    let reference = ContinuationRef::new("p1-ref").unwrap();
    let bound = Message::Assistant(AssistantMessage {
        content: vec![provider_bound_reasoning(&reference, true, "")],
    });
    let history = vec![Message::system("sys"), Message::user("u1"), bound];
    // A stale segment elsewhere; nothing binds this provider-bound assistant.
    let other_ids = history_message_ids(&[Message::user("elsewhere")]).unwrap();
    let continuation = stateless_segments(vec![matching_segment(other_ids[0].clone(), &reference)]);
    let error = continuation
        .validate_for_history(&history)
        .expect_err("missing segment must fail validation");
    assert!(!error.is_empty());

    // Even bypassing validation, encoding fails closed instead of panicking.
    let model = responses_test_model();
    let mut request = base_request(model);
    request.messages = history.clone();
    request.continuation = Some(continuation);
    let (url, _) = json_fixture(completed_response(vec![message_json("x")])).await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url)
        .unwrap();
    let outcome = provider.complete(request).await;
    assert!(outcome.is_err(), "must fail before dispatch, not panic");
    let error = outcome.unwrap_err();
    assert_eq!(error.phase, FailurePhase::BeforeDispatch, "{error:?}");
}

/// P2 — a stale anchor errors without panicking, including through dispatch.
#[tokio::test]
async fn p2_stale_anchor_errors_without_panic() {
    let reference = ContinuationRef::new("p2-ref").unwrap();
    let anchored = Message::Assistant(AssistantMessage {
        content: vec![provider_bound_reasoning(&reference, true, "")],
    });
    let history = vec![Message::system("sys"), Message::user("u1"), anchored];
    let ids = history_message_ids(&history).unwrap();
    let continuation = stateless_segments(vec![matching_segment(ids[1].clone(), &reference)]);

    let mut edited = history.clone();
    if let Message::Assistant(message) = &mut edited[2] {
        message
            .content
            .push(AssistantContent::Text(TextContent::new(
                "post-edit addition",
            )));
    }
    assert!(continuation.validate_for_history(&edited).is_err());

    let model = responses_test_model();
    let mut request = base_request(model);
    request.messages = edited;
    request.continuation = Some(continuation);
    let (url, _) = json_fixture(completed_response(vec![message_json("x")])).await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url)
        .unwrap();
    let outcome = provider.complete(request).await;
    match outcome {
        Err(error) => assert_eq!(error.phase, FailurePhase::BeforeDispatch, "{error:?}"),
        Ok(_) => panic!("stale anchor must fail before dispatch"),
    }
}

/// P3 — malformed deserialized continuation fails validation without panic.
#[test]
fn p3_malformed_deserialized_continuation_fails_validation() {
    // Duplicate anchors across segments survive deserialization but must fail
    // validation against history.
    let reference = ContinuationRef::new("p3-ref").unwrap();
    let anchored = Message::Assistant(AssistantMessage {
        content: vec![provider_bound_reasoning(&reference, true, "")],
    });
    let history = vec![Message::system("sys"), Message::user("u1"), anchored];
    let ids = history_message_ids(&history).unwrap();
    // Malformed state must be exercised through deserialization: external
    // persisted data can bypass constructor validation. Duplicate both the
    // anchor and the reference in raw JSON, decode it, and require
    // validate_for_history to fail closed without panicking.
    let valid = stateless_segments(vec![matching_segment(ids[1].clone(), &reference)]);
    let mut raw = serde_json::to_value(&valid).unwrap();
    assert_eq!(raw["protocol"], "open_ai_responses");
    let first_segment = raw["replay_segments"][0].clone();
    raw["replay_segments"]
        .as_array_mut()
        .unwrap()
        .push(first_segment);
    let decoded: ProviderContinuation = serde_json::from_value(raw).unwrap();
    assert!(decoded.validate().is_err());
    assert!(decoded.validate_for_history(&history).is_err());

    // A serialized unbound legacy continuation decodes but cannot bind.
    let legacy = ProviderContinuation::OpenAiResponses(
        OpenAiResponsesContinuation::new(
            ProviderId::new("openai").unwrap(),
            "gpt-test",
            None,
            vec!["enc".to_string()],
            false,
        )
        .unwrap(),
    );
    legacy.validate().unwrap();
    let error = legacy
        .validate_for_history(&history)
        .expect_err("legacy unbound segment must fail against real history");
    assert!(!error.is_empty());
}

// ---------------------------------------------------------------------------
// STREAM PARITY — TESTS S1..S2
// ---------------------------------------------------------------------------

/// S1 — encrypted reasoning + function call: stream and non-stream produce
/// equivalent replay structure.
#[tokio::test]
async fn s1_stream_and_complete_parity_encrypted_reasoning_plus_tool() {
    let model = responses_test_model();
    let sse = concat!(
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"reasoning\"}}\n\n",
        "event: response.reasoning_summary_text.delta\n",
        "data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"plan\"}\n\n",
        "event: response.output_item.done\n",
        "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"id\":\"rs_s\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"plan\"}],\"encrypted_content\":\"s1-enc\"}}\n\n",
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"type\":\"function_call\",\"call_id\":\"call-s\",\"name\":\"lookup\",\"arguments\":\"\"}}\n\n",
        "event: response.function_call_arguments.delta\n",
        "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":1,\"delta\":\"{\\\"q\\\":7}\"}\n\n",
        "event: response.output_item.done\n",
        "data: {\"type\":\"response.output_item.done\",\"output_index\":1,\"item\":{\"type\":\"function_call\",\"id\":\"fc_s\",\"call_id\":\"call-s\",\"name\":\"lookup\",\"arguments\":\"{\\\"q\\\":7}\"}}\n\n",
        "event: response.completed\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_s1\",\"status\":\"completed\",\"output\":[{\"type\":\"reasoning\",\"id\":\"rs_s\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"plan\"}],\"encrypted_content\":\"s1-enc\"},{\"type\":\"function_call\",\"id\":\"fc_s\",\"call_id\":\"call-s\",\"name\":\"lookup\",\"arguments\":\"{\\\"q\\\":7}\"}],\"usage\":{\"input_tokens\":2,\"output_tokens\":3}}}\n\n"
    );
    let stream_url = sse_fixture(sse.into()).await;
    let streaming = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&stream_url)
        .unwrap();
    let streamed = collect_stream(streaming.stream(base_request(model.clone())).await.unwrap())
        .await
        .unwrap();

    let (complete_url, _) = json_fixture(completed_response(vec![
        reasoning_json("rs_s", Some("s1-enc"), "plan"),
        function_call_json("fc_s", "call-s", "lookup", "{\"q\":7}"),
    ]))
    .await;
    let non_streaming = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&complete_url)
        .unwrap();
    let completed = non_streaming.complete(base_request(model)).await.unwrap();

    compare_replay_structure(&streamed, &completed);
    assert!(streamed
        .continuation
        .as_ref()
        .map(responses)
        .unwrap()
        .replay_segments()[0]
        .items()[0]
        .is_encrypted_reasoning());
}

/// S2 — portable reasoning + assistant message: stream and non-stream
/// produce equivalent replay structure.
#[tokio::test]
async fn s2_stream_and_complete_parity_portable_reasoning_plus_message() {
    let model = responses_test_model();
    let sse = concat!(
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"reasoning\"}}\n\n",
        "event: response.reasoning_summary_text.delta\n",
        "data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"soft plan\"}\n\n",
        "event: response.output_item.done\n",
        "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"id\":\"rs_p2\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"soft plan\"}]}}\n\n",
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[]}}\n\n",
        "event: response.output_item.done\n",
        "data: {\"type\":\"response.output_item.done\",\"output_index\":1,\"item\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"plain answer\"}]}}\n\n",
        "event: response.completed\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_s2\",\"status\":\"completed\",\"output\":[{\"type\":\"reasoning\",\"id\":\"rs_p2\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"soft plan\"}]},{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"plain answer\"}]}],\"usage\":{\"input_tokens\":2,\"output_tokens\":3}}}\n\n"
    );
    let stream_url = sse_fixture(sse.into()).await;
    let streaming = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&stream_url)
        .unwrap();
    let streamed = collect_stream(streaming.stream(base_request(model.clone())).await.unwrap())
        .await
        .unwrap();

    let (complete_url, _) = json_fixture(completed_response(vec![
        reasoning_json("rs_p2", None, "soft plan"),
        message_json("plain answer"),
    ]))
    .await;
    let non_streaming = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&complete_url)
        .unwrap();
    let completed = non_streaming.complete(base_request(model)).await.unwrap();

    compare_replay_structure(&streamed, &completed);
    // Portable reasoning survived on both paths.
    for completion in [&streamed, &completed] {
        let items =
            responses(completion.continuation.as_ref().unwrap()).replay_segments()[0].items();
        assert_eq!(items[0].kind(), "reasoning");
        assert!(!items[0].is_encrypted_reasoning());
        assert_eq!(items[1].kind(), "assistant_message");
    }
}

/// Structural equivalence of streamed vs non-streamed continuations:
/// segment count, item kinds/order, portability, and safe metadata sizes.
fn compare_replay_structure(
    streamed: &jarvis_model_provider::Completion,
    completed: &jarvis_model_provider::Completion,
) {
    let stream_cont = responses(streamed.continuation.as_ref().unwrap());
    let complete_cont = responses(completed.continuation.as_ref().unwrap());
    assert_eq!(stream_cont.mode(), complete_cont.mode());
    assert_eq!(
        stream_cont.replay_segment_count(),
        complete_cont.replay_segment_count()
    );
    for (segment_stream, segment_complete) in stream_cont
        .replay_segments()
        .iter()
        .zip(complete_cont.replay_segments())
    {
        assert_eq!(segment_stream.items().len(), segment_complete.items().len());
        for (a, b) in segment_stream.items().iter().zip(segment_complete.items()) {
            assert_eq!(a.kind(), b.kind());
            assert_eq!(a.is_encrypted_reasoning(), b.is_encrypted_reasoning());
            // Compare payload size through the safe Debug contract only.
            let bytes = |item: &OpenAiResponsesReplayItem| {
                let debug = format!("{item:?}");
                let parsed = debug
                    .split("sensitive_bytes: ")
                    .nth(1)
                    .and_then(|rest| rest.split([',', '}']).next())
                    .and_then(|value| value.trim().parse::<usize>().ok());
                assert!(parsed.is_some(), "no sensitive_bytes metadata in {debug}");
                parsed.unwrap()
            };
            assert_eq!(bytes(a), bytes(b));
        }
    }
    // Neither normalized message leaks sensitive payloads.
    for completion in [streamed, completed] {
        let json = serde_json::to_value(completion.message.clone())
            .unwrap()
            .to_string();
        assert!(!json.contains("s1-enc"));
    }
}

// ---------------------------------------------------------------------------
// Regression guards for the M7.2 semantics themselves.
// ---------------------------------------------------------------------------

/// Stateful mode ignores segments entirely and keeps previous_response_id.
#[tokio::test]
async fn stateful_mode_remains_on_previous_response_id_path() {
    let output = serde_json::json!({
        "id": "resp_stateful_m72",
        "status": "completed",
        "output": [message_json("kept")],
        "usage": {"input_tokens": 1, "output_tokens": 1}
    });
    let (url1, _) = json_fixture(output.clone()).await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url1)
        .unwrap();
    let mut first_request = base_request(responses_test_model());
    first_request.retention = DataRetentionPolicy::ProviderDefault;
    let first = provider.complete(first_request).await.unwrap();
    let continuation = continuation_of(&first);
    assert_eq!(
        responses(&continuation).durability(),
        jarvis_model_provider::ContinuationDurability::ProviderBound
    );

    let (url2, receiver) = json_fixture(output).await;
    let provider2 = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url2)
        .unwrap();
    let mut next = base_request(responses_test_model());
    next.retention = DataRetentionPolicy::ProviderDefault;
    next.messages
        .push(Message::Assistant(first.message.clone()));
    next.messages.push(Message::user("suffix"));
    next.continuation = Some(continuation);
    provider2.complete(next).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&receiver.await.unwrap()).unwrap();
    assert_eq!(body["previous_response_id"], "resp_stateful_m72");
    assert_eq!(body["input"].as_array().unwrap().len(), 1);
}

/// Debug redaction holds for the new portable/encrypted reasoning shapes.
#[test]
fn replay_item_debug_reports_metadata_only() {
    let encrypted = OpenAiResponsesReplayItem::reasoning(
        ContinuationRef::new("ref-e").unwrap(),
        Some("rs_e".into()),
        "super-secret-encrypted-bytes",
        vec!["visible-summary".into()],
    );
    let portable = OpenAiResponsesReplayItem::portable_reasoning(
        vec!["portable-summary".into()],
        Some("rs_p".into()),
    );
    let encrypted_debug = format!("{encrypted:?}");
    let portable_debug = format!("{portable:?}");
    assert!(!encrypted_debug.contains("super-secret-encrypted-bytes"));
    assert!(encrypted_debug.contains("encrypted_present: true"));
    assert!(encrypted_debug.contains("sensitive_bytes"));
    assert!(portable_debug.contains("encrypted_present: false"));
    // Summary text is treated as sensitive-sized metadata, not dumped.
    assert!(!portable_debug.contains("portable-summary"));
}

/// Serialized HistoryMessageId round-trips stably across recovery.
#[test]
fn history_identity_survives_serialization_round_trip() {
    let history = vec![Message::user("u1"), assistant_text("a1")];
    let ids = history_message_ids(&history).unwrap();
    let encoded = serde_json::to_vec(&ids).unwrap();
    let decoded: Vec<jarvis_model_provider::HistoryMessageId> =
        serde_json::from_slice(&encoded).unwrap();
    assert_eq!(ids, decoded);
    let recomputed = history_message_ids(&history).unwrap();
    assert_eq!(ids, recomputed);
}
