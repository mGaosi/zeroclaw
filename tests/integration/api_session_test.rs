//! Integration tests for API session persistence.
//! Covers T008, T012, T016, T019 from tasks.md.

use crate::support::api_helpers::{build_test_handle_with_config, collect_events, text_response};
use crate::support::MockProvider;
use tempfile::TempDir;
use zeroclaw::api::conversation;
use zeroclaw::api::types::{ApiError, ConfigPatch};
use zeroclaw::config::Config;

/// Helper to build a Config with session persistence enabled and custom workspace.
fn config_with_workspace(workspace: &std::path::Path) -> Config {
    let mut config = Config::default();
    config.workspace_dir = workspace.to_path_buf();
    config.channels_config.session_persistence = true;
    config.channels_config.session_backend = "jsonl".into();
    config
}

/// Helper to build a test handle with session persistence.
async fn build_session_handle(
    workspace: &std::path::Path,
    responses: Vec<zeroclaw::providers::ChatResponse>,
) -> zeroclaw::api::lifecycle::AgentHandle {
    let provider = Box::new(MockProvider::new(responses));
    let config = config_with_workspace(workspace);
    let handle = build_test_handle_with_config(provider, vec![], config, None);
    // Initialize JSONL session backend (from_agent_for_test leaves it as None).
    let backend =
        zeroclaw::channels::session_store::SessionStore::new(workspace).expect("session store");
    let sb = handle.session_backend();
    let mut guard = sb.write().await;
    *guard = Some(std::sync::Arc::new(backend));
    drop(guard);
    handle
}

// ── T008: US1 Integration tests — workspace directory ──

/// Init with a valid tempdir workspace succeeds and session backend is initialized.
#[tokio::test(flavor = "multi_thread")]
async fn test_init_with_custom_workspace_dir() {
    let tmp = TempDir::new().unwrap();
    let handle = build_session_handle(tmp.path(), vec![text_response("hello")]).await;
    assert!(handle.is_initialized());
}

/// ConfigPatch workspace_dir applies correctly via init flow.
#[tokio::test(flavor = "multi_thread")]
async fn test_workspace_dir_applied_via_config_patch() {
    let tmp = TempDir::new().unwrap();
    let mut config = Config::default();
    config.channels_config.session_persistence = true;
    config.channels_config.session_backend = "jsonl".into();

    let patch = ConfigPatch {
        workspace_dir: Some(tmp.path().to_string_lossy().into_owned()),
        ..Default::default()
    };
    patch.apply_to(&mut config);
    assert_eq!(config.workspace_dir, tmp.path().to_path_buf());
}

/// Init without workspace_dir (default behavior) doesn't crash.
#[tokio::test(flavor = "multi_thread")]
async fn test_init_default_workspace_fallback() {
    let provider = Box::new(MockProvider::new(vec![text_response("ok")]));
    let handle = build_test_handle_with_config(provider, vec![], Config::default(), None);
    assert!(handle.is_initialized());
}

// ── T012: US2 Integration tests — session persistence ──

/// Sending a message with a session key creates a session file.
#[tokio::test(flavor = "multi_thread")]
async fn test_send_message_persists_conversation() {
    let tmp = TempDir::new().unwrap();
    let handle = build_session_handle(
        tmp.path(),
        vec![text_response("response1"), text_response("response2")],
    )
    .await;

    let (tx, rx) = tokio::sync::mpsc::channel(64);
    conversation::send_message(&handle, "hello".into(), Some("test_session".into()), tx).unwrap();
    let events = collect_events(rx).await;
    assert!(events
        .iter()
        .any(|e| matches!(e, zeroclaw::api::types::StreamEvent::Done { .. })));

    // Verify session file exists on disk.
    let session_path = tmp.path().join("sessions").join("test_session.jsonl");
    // Give async task time to persist.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        session_path.exists(),
        "Session file should exist at {:?}",
        session_path
    );
}

/// Default session key ("api_default") is used when None is passed.
#[tokio::test(flavor = "multi_thread")]
async fn test_default_session_key() {
    let tmp = TempDir::new().unwrap();
    let handle = build_session_handle(tmp.path(), vec![text_response("hi")]).await;

    let (tx, rx) = tokio::sync::mpsc::channel(64);
    conversation::send_message(&handle, "hello".into(), None, tx).unwrap();
    let _events = collect_events(rx).await;

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let session_path = tmp.path().join("sessions").join("api_default.jsonl");
    assert!(
        session_path.exists(),
        "Default session file should exist at {:?}",
        session_path
    );
}

/// Invalid session key (with path separator) is rejected.
#[tokio::test(flavor = "multi_thread")]
async fn test_invalid_session_key_rejected() {
    let tmp = TempDir::new().unwrap();
    let handle = build_session_handle(tmp.path(), vec![text_response("hi")]).await;

    let (tx, _rx) = tokio::sync::mpsc::channel(64);
    let result = conversation::send_message(&handle, "hello".into(), Some("../evil".into()), tx);
    assert!(matches!(result, Err(ApiError::ValidationError { .. })));
}

/// Switching session keys clears and reloads.
#[tokio::test(flavor = "multi_thread")]
async fn test_session_switch_clears_and_reloads() {
    let tmp = TempDir::new().unwrap();
    let handle = build_session_handle(
        tmp.path(),
        vec![text_response("response_a"), text_response("response_b")],
    )
    .await;

    // Send to chat_1.
    let (tx1, rx1) = tokio::sync::mpsc::channel(64);
    conversation::send_message(&handle, "msg_a".into(), Some("chat_1".into()), tx1).unwrap();
    let _events1 = collect_events(rx1).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Send to chat_2 — should not contain chat_1's history.
    let (tx2, rx2) = tokio::sync::mpsc::channel(64);
    conversation::send_message(&handle, "msg_b".into(), Some("chat_2".into()), tx2).unwrap();
    let _events2 = collect_events(rx2).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Both session files should exist.
    assert!(tmp.path().join("sessions").join("chat_1.jsonl").exists());
    assert!(tmp.path().join("sessions").join("chat_2.jsonl").exists());
}

// ── T016: US3 Integration tests — session management ──

/// list_sessions returns metadata for created sessions.
#[tokio::test(flavor = "multi_thread")]
async fn test_list_sessions_returns_metadata() {
    let tmp = TempDir::new().unwrap();
    let handle = build_session_handle(
        tmp.path(),
        vec![
            text_response("r1"),
            text_response("r2"),
            text_response("r3"),
        ],
    )
    .await;

    // Create 3 sessions.
    for key in &["session_a", "session_b", "session_c"] {
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        conversation::send_message(&handle, "hi".into(), Some(key.to_string()), tx).unwrap();
        let _ = collect_events(rx).await;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let sessions = conversation::list_sessions(&handle).await.unwrap();
    assert!(
        sessions.len() >= 3,
        "Expected at least 3 sessions, got {}",
        sessions.len()
    );
}

/// load_session_history returns messages for existing session.
#[tokio::test(flavor = "multi_thread")]
async fn test_load_session_history() {
    let tmp = TempDir::new().unwrap();
    let handle = build_session_handle(tmp.path(), vec![text_response("world")]).await;

    let (tx, rx) = tokio::sync::mpsc::channel(64);
    conversation::send_message(&handle, "hello".into(), Some("load_test".into()), tx).unwrap();
    let _ = collect_events(rx).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let history = conversation::load_session_history(&handle, "load_test".into())
        .await
        .unwrap();
    assert!(!history.is_empty(), "History should contain messages");
}

/// load_session_history for non-existent session returns empty vec.
#[tokio::test(flavor = "multi_thread")]
async fn test_load_nonexistent_session_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let handle = build_session_handle(tmp.path(), vec![]).await;

    let history = conversation::load_session_history(&handle, "nonexistent".into())
        .await
        .unwrap();
    assert!(history.is_empty());
}

/// delete_session removes a session.
#[tokio::test(flavor = "multi_thread")]
async fn test_delete_session() {
    let tmp = TempDir::new().unwrap();
    let handle = build_session_handle(tmp.path(), vec![text_response("bye")]).await;

    let (tx, rx) = tokio::sync::mpsc::channel(64);
    conversation::send_message(&handle, "hello".into(), Some("to_delete".into()), tx).unwrap();
    let _ = collect_events(rx).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Verify file exists before delete.
    let session_path = tmp.path().join("sessions").join("to_delete.jsonl");
    assert!(
        session_path.exists(),
        "Session file should exist before delete"
    );

    conversation::delete_session(&handle, "to_delete".into())
        .await
        .unwrap();

    // Verify file was removed.
    assert!(
        !session_path.exists(),
        "Session file should be removed after delete"
    );

    let sessions = conversation::list_sessions(&handle).await.unwrap();
    assert!(
        !sessions.iter().any(|s| s.key == "to_delete"),
        "Deleted session should not appear in list, found: {:?}",
        sessions.iter().map(|s| &s.key).collect::<Vec<_>>()
    );
}

/// delete with empty key returns validation error.
#[tokio::test(flavor = "multi_thread")]
async fn test_delete_empty_key_returns_validation_error() {
    let tmp = TempDir::new().unwrap();
    let handle = build_session_handle(tmp.path(), vec![]).await;

    let result = conversation::delete_session(&handle, String::new()).await;
    assert!(matches!(result, Err(ApiError::ValidationError { .. })));
}

// ── T019: US4 Integration tests — runtime workspace change ──

/// After switching the session backend to a new workspace, sessions persist in the new location.
#[tokio::test(flavor = "multi_thread")]
async fn test_runtime_workspace_change() {
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();
    let handle = build_session_handle(
        tmp_a.path(),
        vec![text_response("in_a"), text_response("in_b")],
    )
    .await;

    // Send to workspace A.
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    conversation::send_message(&handle, "msg".into(), Some("ws_test".into()), tx).unwrap();
    let _ = collect_events(rx).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(tmp_a.path().join("sessions").join("ws_test.jsonl").exists());

    // Simulate runtime workspace change: replace session backend with one for workspace B.
    let new_backend =
        zeroclaw::channels::session_store::SessionStore::new(tmp_b.path()).expect("session store");
    {
        let sb = handle.session_backend();
        let mut guard = sb.write().await;
        *guard = Some(std::sync::Arc::new(new_backend));
    }
    // Reset current_session_key so next send triggers reload.
    {
        let csk = handle.current_session_key();
        let mut guard = csk.lock().await;
        *guard = None;
    }

    // Send to workspace B.
    let (tx2, rx2) = tokio::sync::mpsc::channel(64);
    conversation::send_message(&handle, "msg2".into(), Some("ws_test_b".into()), tx2).unwrap();
    let _ = collect_events(rx2).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Session should now be in the new workspace B.
    assert!(tmp_b
        .path()
        .join("sessions")
        .join("ws_test_b.jsonl")
        .exists());
}
