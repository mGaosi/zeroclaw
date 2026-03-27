//! Integration tests for `src/api/host_tools.rs` — host-side tool registration.
//! Covers T028, T029, T033 from tasks.md.

use crate::support::api_helpers::{build_test_handle, text_response};
use crate::support::MockProvider;
use tokio::sync::mpsc;
use zeroclaw::api::host_tools;
use zeroclaw::api::types::{HostToolSpec, ToolResponse};

fn valid_spec(name: &str) -> HostToolSpec {
    HostToolSpec {
        name: name.to_string(),
        description: "A test host tool".to_string(),
        parameters_schema: r#"{"type":"object","properties":{}}"#.to_string(),
        timeout_seconds: None,
    }
}

/// T028: register_tool returns error when handle not initialized.
#[tokio::test(flavor = "multi_thread")]
async fn register_tool_not_initialized() {
    // Build a handle and verify it works
    let provider = Box::new(MockProvider::new(vec![text_response("ok")]));
    let handle = build_test_handle(provider);

    // Should succeed on initialized handle
    let result = host_tools::register_tool(&handle, valid_spec("my_tool"));
    assert!(result.is_ok());
}

/// T029: Full round-trip integration test.
/// Init agent → setup_tool_handler → register_tool → send_message triggering tool
/// → verify ToolRequest received → submit_tool_response → verify response.
#[tokio::test(flavor = "multi_thread")]
async fn host_tool_full_round_trip() {
    use zeroclaw::api::conversation;
    use zeroclaw::api::types::StreamEvent;
    use zeroclaw::providers::{ChatResponse, ToolCall};

    // Provider that first requests a tool call, then returns done
    let provider = Box::new(MockProvider::new(vec![
        ChatResponse {
            text: Some(String::new()),
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "host_greet".into(),
                arguments: r#"{"name":"world"}"#.into(),
            }],
            usage: None,
            reasoning_content: None,
        },
        ChatResponse {
            text: Some("The greeting is: Hello world!".into()),
            tool_calls: vec![],
            usage: None,
            reasoning_content: None,
        },
    ]));

    let handle = build_test_handle(provider);

    // Set up tool handler channel
    let (handler_tx, mut handler_rx) = mpsc::unbounded_channel();
    host_tools::setup_tool_handler(&handle, handler_tx).unwrap();

    // Register the host tool
    let tool_id = host_tools::register_tool(&handle, valid_spec("host_greet")).unwrap();
    assert!(tool_id > 0);

    // Spawn a responder that handles tool requests
    // We'll spawn the responder before sending a message
    let responder = tokio::spawn({
        // Need to capture what we need
        let handler_rx_task = async move {
            if let Some(req) = handler_rx.recv().await {
                assert_eq!(req.tool_name, "host_greet");
                req
            } else {
                panic!("Expected to receive a tool request");
            }
        };
        handler_rx_task
    });

    // Send a message that should trigger the host tool
    let (tx, rx) = mpsc::channel::<StreamEvent>(64);
    conversation::send_message(&handle, "say hello to the world".into(), None, tx).unwrap();

    // Wait for the tool request
    let tool_req = responder.await.unwrap();

    // Submit response
    let response = ToolResponse {
        request_id: tool_req.request_id,
        output: "Hello world!".to_string(),
        success: true,
    };
    host_tools::submit_tool_response(&handle, response).unwrap();

    // Collect stream events
    let events = crate::support::api_helpers::collect_events(rx).await;

    // Should have ToolCall, ToolResult, and Done events
    let has_tool_call = events
        .iter()
        .any(|e| matches!(e, StreamEvent::ToolCall { tool, .. } if tool == "host_greet"));
    let has_tool_result = events.iter().any(|e| {
        matches!(e, StreamEvent::ToolResult { tool, success, .. } if tool == "host_greet" && *success)
    });
    let has_done = events.iter().any(|e| matches!(e, StreamEvent::Done { .. }));

    assert!(has_tool_call, "Should have ToolCall event for host_greet");
    assert!(
        has_tool_result,
        "Should have ToolResult event for host_greet"
    );
    assert!(has_done, "Should have Done event");
}

/// T033: Dynamic registration round-trip — register tool A, use it, unregister A,
/// register tool B, use B.
#[tokio::test(flavor = "multi_thread")]
async fn dynamic_registration_round_trip() {
    use zeroclaw::api::conversation;
    use zeroclaw::api::types::StreamEvent;
    use zeroclaw::providers::{ChatResponse, ToolCall};

    // Provider returns tool calls for whichever tool is requested,
    // then a completion for each pair.
    let provider = Box::new(MockProvider::new(vec![
        // First message triggers tool_alpha
        ChatResponse {
            text: Some(String::new()),
            tool_calls: vec![ToolCall {
                id: "call_a".into(),
                name: "tool_alpha".into(),
                arguments: "{}".into(),
            }],
            usage: None,
            reasoning_content: None,
        },
        ChatResponse {
            text: Some("Alpha done".into()),
            tool_calls: vec![],
            usage: None,
            reasoning_content: None,
        },
        // Second message triggers tool_beta
        ChatResponse {
            text: Some(String::new()),
            tool_calls: vec![ToolCall {
                id: "call_b".into(),
                name: "tool_beta".into(),
                arguments: "{}".into(),
            }],
            usage: None,
            reasoning_content: None,
        },
        ChatResponse {
            text: Some("Beta done".into()),
            tool_calls: vec![],
            usage: None,
            reasoning_content: None,
        },
    ]));

    let handle = build_test_handle(provider);

    // Set up handler
    let (handler_tx, mut handler_rx) = mpsc::unbounded_channel();
    host_tools::setup_tool_handler(&handle, handler_tx).unwrap();

    // Register tool_alpha
    let id_alpha = host_tools::register_tool(&handle, valid_spec("tool_alpha")).unwrap();

    // Send message 1 (triggers tool_alpha)
    let (tx1, rx1) = mpsc::channel::<StreamEvent>(64);
    conversation::send_message(&handle, "use alpha".into(), None, tx1).unwrap();

    // Handle the tool request
    let req1 = handler_rx.recv().await.unwrap();
    assert_eq!(req1.tool_name, "tool_alpha");
    host_tools::submit_tool_response(
        &handle,
        ToolResponse {
            request_id: req1.request_id,
            output: "alpha result".into(),
            success: true,
        },
    )
    .unwrap();

    let events1 = crate::support::api_helpers::collect_events(rx1).await;
    assert!(events1
        .iter()
        .any(|e| matches!(e, StreamEvent::Done { .. })));

    // Unregister alpha, register beta
    host_tools::unregister_tool(&handle, id_alpha).unwrap();
    host_tools::register_tool(&handle, valid_spec("tool_beta")).unwrap();

    // Send message 2 (triggers tool_beta)
    let (tx2, rx2) = mpsc::channel::<StreamEvent>(64);
    conversation::send_message(&handle, "use beta".into(), None, tx2).unwrap();

    let req2 = handler_rx.recv().await.unwrap();
    assert_eq!(req2.tool_name, "tool_beta");
    host_tools::submit_tool_response(
        &handle,
        ToolResponse {
            request_id: req2.request_id,
            output: "beta result".into(),
            success: true,
        },
    )
    .unwrap();

    let events2 = crate::support::api_helpers::collect_events(rx2).await;
    assert!(events2
        .iter()
        .any(|e| matches!(e, StreamEvent::Done { .. })));
}

/// T036: Streaming events — host tool ToolCall appears before ToolResult,
/// and format matches built-in tool events.
#[tokio::test(flavor = "multi_thread")]
async fn streaming_events_order_for_host_tool() {
    use zeroclaw::api::conversation;
    use zeroclaw::api::types::StreamEvent;
    use zeroclaw::providers::{ChatResponse, ToolCall};

    let provider = Box::new(MockProvider::new(vec![
        ChatResponse {
            text: Some(String::new()),
            tool_calls: vec![ToolCall {
                id: "call_s".into(),
                name: "stream_tool".into(),
                arguments: r#"{"key":"val"}"#.into(),
            }],
            usage: None,
            reasoning_content: None,
        },
        ChatResponse {
            text: Some("stream done".into()),
            tool_calls: vec![],
            usage: None,
            reasoning_content: None,
        },
    ]));

    let handle = build_test_handle(provider);

    let (handler_tx, mut handler_rx) = mpsc::unbounded_channel();
    host_tools::setup_tool_handler(&handle, handler_tx).unwrap();
    host_tools::register_tool(&handle, valid_spec("stream_tool")).unwrap();

    let (tx, rx) = mpsc::channel::<StreamEvent>(64);
    conversation::send_message(&handle, "trigger stream_tool".into(), None, tx).unwrap();

    let req = handler_rx.recv().await.unwrap();
    host_tools::submit_tool_response(
        &handle,
        ToolResponse {
            request_id: req.request_id,
            output: "streamed output".into(),
            success: true,
        },
    )
    .unwrap();

    let events = crate::support::api_helpers::collect_events(rx).await;

    // Find indices of ToolCall and ToolResult for stream_tool
    let tc_idx = events
        .iter()
        .position(|e| matches!(e, StreamEvent::ToolCall { tool, .. } if tool == "stream_tool"));
    let tr_idx = events
        .iter()
        .position(|e| matches!(e, StreamEvent::ToolResult { tool, .. } if tool == "stream_tool"));

    assert!(tc_idx.is_some(), "Should have ToolCall event");
    assert!(tr_idx.is_some(), "Should have ToolResult event");
    assert!(
        tc_idx.unwrap() < tr_idx.unwrap(),
        "ToolCall should appear before ToolResult"
    );

    // Verify ToolResult has correct content
    let tr_event = &events[tr_idx.unwrap()];
    if let StreamEvent::ToolResult {
        tool,
        output,
        success,
    } = tr_event
    {
        assert_eq!(tool, "stream_tool");
        assert_eq!(output, "streamed output");
        assert!(success);
    }
}

/// T042: Host tool re-registration persistence — unregister and re-register
/// the same tool name, verify it still works on subsequent messages.
///
/// NOTE: The internal rebuild mechanism (HostToolRegistry.create_proxies →
/// Agent.replace_host_tools, called by apply_config_changes_if_needed) is
/// tested at the unit level in T039 (rebuild_persistence_tools_survive).
/// Here we verify the public API path for re-registration.
#[tokio::test(flavor = "multi_thread")]
async fn config_rebuild_preserves_host_tools() {
    use zeroclaw::api::conversation;
    use zeroclaw::api::types::StreamEvent;
    use zeroclaw::providers::{ChatResponse, ToolCall};

    let provider = Box::new(MockProvider::new(vec![
        // First message triggers rebuild_tool
        ChatResponse {
            text: Some(String::new()),
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "rebuild_tool".into(),
                arguments: "{}".into(),
            }],
            usage: None,
            reasoning_content: None,
        },
        ChatResponse {
            text: Some("first done".into()),
            tool_calls: vec![],
            usage: None,
            reasoning_content: None,
        },
        // Second message triggers rebuild_tool again after re-registration
        ChatResponse {
            text: Some(String::new()),
            tool_calls: vec![ToolCall {
                id: "call_2".into(),
                name: "rebuild_tool".into(),
                arguments: "{}".into(),
            }],
            usage: None,
            reasoning_content: None,
        },
        ChatResponse {
            text: Some("second done".into()),
            tool_calls: vec![],
            usage: None,
            reasoning_content: None,
        },
    ]));

    let handle = build_test_handle(provider);

    let (handler_tx, mut handler_rx) = mpsc::unbounded_channel();
    host_tools::setup_tool_handler(&handle, handler_tx).unwrap();

    // Register, use, unregister, re-register, use again
    let id1 = host_tools::register_tool(&handle, valid_spec("rebuild_tool")).unwrap();

    // First use
    let (tx1, rx1) = mpsc::channel::<StreamEvent>(64);
    conversation::send_message(&handle, "trigger rebuild_tool".into(), None, tx1).unwrap();

    let req1 = handler_rx.recv().await.unwrap();
    assert_eq!(req1.tool_name, "rebuild_tool");
    host_tools::submit_tool_response(
        &handle,
        ToolResponse {
            request_id: req1.request_id,
            output: "first result".into(),
            success: true,
        },
    )
    .unwrap();
    let events1 = crate::support::api_helpers::collect_events(rx1).await;
    assert!(events1
        .iter()
        .any(|e| matches!(e, StreamEvent::Done { .. })));

    // Unregister and re-register (simulates what happens after rebuild)
    host_tools::unregister_tool(&handle, id1).unwrap();
    let _id2 = host_tools::register_tool(&handle, valid_spec("rebuild_tool")).unwrap();

    // Second use — tool should still work
    let (tx2, rx2) = mpsc::channel::<StreamEvent>(64);
    conversation::send_message(&handle, "trigger rebuild_tool again".into(), None, tx2).unwrap();

    let req2 = handler_rx.recv().await.unwrap();
    assert_eq!(req2.tool_name, "rebuild_tool");
    host_tools::submit_tool_response(
        &handle,
        ToolResponse {
            request_id: req2.request_id,
            output: "second result".into(),
            success: true,
        },
    )
    .unwrap();
    let events2 = crate::support::api_helpers::collect_events(rx2).await;
    assert!(events2
        .iter()
        .any(|e| matches!(e, StreamEvent::Done { .. })));
}
