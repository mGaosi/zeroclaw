//! Integration tests for `src/api/conversation.rs` — send_message() and cancel_message().
//! Covers T009, T010 from tasks.md (US1).

use crate::support::api_helpers::{
    build_test_handle, build_test_handle_with_tools, collect_events, text_response,
};
use crate::support::{EchoTool, MockProvider};
use zeroclaw::api::conversation;
use zeroclaw::api::types::StreamEvent;
use zeroclaw::providers::{ChatResponse, ToolCall};

/// T009: send_message() round-trip delivers Chunk + Done events in correct order.
#[tokio::test(flavor = "multi_thread")]
async fn send_message_text_roundtrip() {
    let provider = Box::new(MockProvider::new(vec![text_response(
        "Hello from the agent!",
    )]));
    let handle = build_test_handle(provider);

    let (tx, rx) = tokio::sync::mpsc::channel(32);
    conversation::send_message(&handle, "hi".into(), None, tx).unwrap();

    let events = collect_events(rx).await;
    assert!(!events.is_empty(), "should have received events");

    // Last event must be Done
    let last = events.last().unwrap();
    assert!(
        matches!(last, StreamEvent::Done { .. }),
        "last event should be Done, got: {last:?}"
    );

    // Must have at least one Chunk before Done
    let chunks: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, StreamEvent::Chunk { .. }))
        .collect();
    assert!(!chunks.is_empty(), "should have at least one Chunk event");

    // Done should contain the full response
    if let StreamEvent::Done { full_response } = last {
        assert!(
            !full_response.is_empty(),
            "Done full_response should not be empty"
        );
    }
}

/// T009 supplement: send_message with empty message returns validation error.
#[tokio::test(flavor = "multi_thread")]
async fn send_message_empty_message_errors() {
    let provider = Box::new(MockProvider::new(vec![]));
    let handle = build_test_handle(provider);

    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    let result = conversation::send_message(&handle, String::new(), None, tx);
    assert!(result.is_err());
}

/// T010: send_message() with tool-calling agent delivers ToolCall + ToolResult events.
#[tokio::test(flavor = "multi_thread")]
async fn send_message_with_tool_calls() {
    // First response: provider requests a tool call
    let tool_call_response = ChatResponse {
        text: Some(String::new()),
        tool_calls: vec![ToolCall {
            id: "call_1".into(),
            name: "echo".into(),
            arguments: r#"{"message": "test echo"}"#.into(),
        }],
        usage: None,
        reasoning_content: None,
    };
    // Second response: provider gives final text (after tool result)
    let final_response = text_response("Tool result received, here is my answer.");

    let provider = Box::new(MockProvider::new(vec![tool_call_response, final_response]));
    let handle = build_test_handle_with_tools(provider, vec![Box::new(EchoTool)]);

    let (tx, rx) = tokio::sync::mpsc::channel(32);
    conversation::send_message(&handle, "use a tool".into(), None, tx).unwrap();

    let events = collect_events(rx).await;

    // Should contain ToolCall and ToolResult events
    let has_tool_call = events
        .iter()
        .any(|e| matches!(e, StreamEvent::ToolCall { .. }));
    let has_tool_result = events
        .iter()
        .any(|e| matches!(e, StreamEvent::ToolResult { .. }));
    assert!(has_tool_call, "should have a ToolCall event");
    assert!(has_tool_result, "should have a ToolResult event");

    // ToolCall should appear before ToolResult
    let tc_pos = events
        .iter()
        .position(|e| matches!(e, StreamEvent::ToolCall { .. }))
        .unwrap();
    let tr_pos = events
        .iter()
        .position(|e| matches!(e, StreamEvent::ToolResult { .. }))
        .unwrap();
    assert!(tc_pos < tr_pos, "ToolCall should precede ToolResult");

    // Last event must be Done
    assert!(matches!(events.last().unwrap(), StreamEvent::Done { .. }));
}
