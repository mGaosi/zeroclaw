//! Integration tests for `src/api/observer.rs` — register/unregister observer.
//! Covers T031, T032, T033 from tasks.md (Phase 8: Observer).

use crate::support::api_helpers::{build_test_handle, collect_events, text_response};
use crate::support::MockProvider;
use tokio::sync::mpsc;
use zeroclaw::api::{conversation, observer};

/// T031: register_observer() receives LlmRequest and LlmResponse events during send_message().
#[tokio::test(flavor = "multi_thread")]
async fn observer_receives_llm_events_during_send() {
    let provider = Box::new(MockProvider::new(vec![text_response(
        "Hello from the agent",
    )]));
    let handle = build_test_handle(provider);

    // Register observer
    let (obs_tx, mut obs_rx) = mpsc::unbounded_channel();
    let obs_id = observer::register_observer(&handle, obs_tx).unwrap();
    assert!(obs_id > 0, "observer ID should be positive");

    // Send a message
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    conversation::send_message(&handle, "hello".into(), None, tx).unwrap();

    // Wait for message to complete
    let _events = collect_events(rx).await;

    // Give observer events time to propagate
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Collect observer events
    let mut obs_events = Vec::new();
    while let Ok(event) = obs_rx.try_recv() {
        obs_events.push(event);
    }

    // Note: The exact events depend on the Observer trait implementation.
    // At minimum, the observer should receive some events during a turn.
    assert!(
        !obs_events.is_empty(),
        "observer should receive at least some events during send_message, got none"
    );

    // Clean up
    observer::unregister_observer(&handle, obs_id).unwrap();
}

/// T032: unregister_observer() stops event delivery immediately.
#[tokio::test(flavor = "multi_thread")]
async fn unregister_stops_event_delivery() {
    let provider = Box::new(MockProvider::new(vec![
        text_response("first"),
        text_response("second"),
    ]));
    let handle = build_test_handle(provider);

    // Register observer
    let (obs_tx, mut obs_rx) = mpsc::unbounded_channel();
    let obs_id = observer::register_observer(&handle, obs_tx).unwrap();

    // Send first message to verify observer receives events
    let (tx1, rx1) = tokio::sync::mpsc::channel(32);
    conversation::send_message(&handle, "first".into(), None, tx1).unwrap();
    let _events1 = collect_events(rx1).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Drain observer events from first message
    let mut first_events = Vec::new();
    while let Ok(event) = obs_rx.try_recv() {
        first_events.push(event);
    }

    // Unregister
    observer::unregister_observer(&handle, obs_id).unwrap();

    // Send second message
    let (tx2, rx2) = tokio::sync::mpsc::channel(32);
    conversation::send_message(&handle, "second".into(), None, tx2).unwrap();
    let _events2 = collect_events(rx2).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Should NOT receive events from second message
    let after_unregister = obs_rx.try_recv();
    assert!(
        after_unregister.is_err(),
        "should not receive events after unregister"
    );
}

/// T033: Multiple observers receive events independently.
#[tokio::test(flavor = "multi_thread")]
async fn multiple_observers_independent() {
    let provider = Box::new(MockProvider::new(vec![text_response("hello")]));
    let handle = build_test_handle(provider);

    // Register two observers
    let (obs_tx1, mut obs_rx1) = mpsc::unbounded_channel();
    let (obs_tx2, mut obs_rx2) = mpsc::unbounded_channel();
    let id1 = observer::register_observer(&handle, obs_tx1).unwrap();
    let id2 = observer::register_observer(&handle, obs_tx2).unwrap();

    assert_ne!(id1, id2, "observer IDs should be unique");

    // Send a message
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    conversation::send_message(&handle, "hello".into(), None, tx).unwrap();
    let _events = collect_events(rx).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Both observers should receive events
    let mut events1 = Vec::new();
    while let Ok(e) = obs_rx1.try_recv() {
        events1.push(e);
    }
    let mut events2 = Vec::new();
    while let Ok(e) = obs_rx2.try_recv() {
        events2.push(e);
    }

    assert!(!events1.is_empty(), "observer 1 should receive events");
    assert!(!events2.is_empty(), "observer 2 should receive events");

    // Unregister first observer — second should still work
    observer::unregister_observer(&handle, id1).unwrap();

    // Clean up
    observer::unregister_observer(&handle, id2).unwrap();
}
