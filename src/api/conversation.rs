use crate::agent::agent::Agent;
use crate::api::lifecycle::AgentHandle;
use crate::api::types::{ApiError, StreamEvent};
use crate::providers::traits::{ChatMessage, ConversationMessage};
use tokio::sync::mpsc;

#[cfg(feature = "frb")]
use flutter_rust_bridge::StreamSink;

/// Convert `ConversationMessage` variants to `ChatMessage` records for persistence.
///
/// - `Chat(m)` → as-is
/// - `AssistantToolCalls { text, tool_calls, reasoning_content }` → JSON-serialized assistant ChatMessage
/// - `ToolResults(results)` → one tool ChatMessage per result
pub fn conversation_messages_to_chat_messages(
    messages: &[ConversationMessage],
) -> Vec<ChatMessage> {
    let mut out = Vec::new();
    for msg in messages {
        match msg {
            ConversationMessage::Chat(m) => {
                out.push(m.clone());
            }
            ConversationMessage::AssistantToolCalls {
                text,
                tool_calls,
                reasoning_content,
            } => {
                let payload = serde_json::json!({
                    "text": text,
                    "tool_calls": tool_calls,
                    "reasoning_content": reasoning_content,
                });
                out.push(ChatMessage::assistant(payload.to_string()));
            }
            ConversationMessage::ToolResults(results) => {
                for result in results {
                    let payload = serde_json::json!({
                        "tool_call_id": result.tool_call_id,
                        "content": result.content,
                    });
                    out.push(ChatMessage::tool(payload.to_string()));
                }
            }
        }
    }
    out
}

/// Send a message to the agent and receive streaming events.
///
/// Spawns an async task that pushes events into the returned receiver.
/// The function returns a receiver immediately; events arrive as they are
/// produced. The stream closes after a `Done` or `Error` event.
///
/// When `session_key` is `None`, defaults to `"api_default"`. Session history
/// is automatically loaded on first use or when switching keys, and new
/// messages are persisted after each turn.
///
/// `images` may contain data-URIs (`data:image/png;base64,...`) or file paths.
/// They are injected as `[IMAGE:...]` markers appended to the message text,
/// which the existing multimodal pipeline handles transparently.
pub fn send_message(
    handle: &AgentHandle,
    message: String,
    session_key: Option<String>,
    images: Option<Vec<String>>,
    tx: mpsc::Sender<StreamEvent>,
) -> Result<(), ApiError> {
    if !handle.is_initialized() {
        return Err(ApiError::NotInitialized);
    }
    if message.is_empty() {
        return Err(ApiError::ValidationError {
            message: "message must not be empty".into(),
        });
    }

    // Inject image markers into the message text so the multimodal pipeline picks them up.
    let message = if let Some(ref imgs) = images {
        use std::fmt::Write;
        let mut buf = message;
        for img in imgs {
            let _ = write!(buf, "\n[IMAGE:{img}]");
        }
        buf
    } else {
        message
    };

    let effective_key = session_key.unwrap_or_else(|| "api_default".into());
    crate::api::types::validate_session_key(&effective_key)?;

    let agent = handle.agent();
    let cancel_token = handle.cancel_token();
    let observer = handle.observer_registry();
    let config_manager = handle.config_manager();
    let config_rx = handle.config_rx();
    let host_tool_registry = handle.host_tool_registry();
    let session_backend = handle.session_backend();
    let current_session_key = handle.current_session_key();

    tokio::spawn(async move {
        // T029: Check for config changes and rebuild agent if needed
        if let Err(e) = Box::pin(apply_config_changes_if_needed(
            &agent,
            &config_manager,
            &config_rx,
            &host_tool_registry,
            &cancel_token,
            &session_backend,
            &current_session_key,
        ))
        .await
        {
            tracing::warn!("Failed to apply config changes: {e}");
        }

        // Session switching: compare effective key against current.
        let backend_guard = session_backend.read().await;
        if let Some(ref backend) = *backend_guard {
            let mut key_guard = current_session_key.lock().await;
            let needs_switch = key_guard.as_deref() != Some(&effective_key);

            if needs_switch {
                // Clear in-memory history (post-turn persistence already saved new messages).
                {
                    let mut guard = agent.lock().await;
                    guard.clear_history();
                }

                // Load new session history if it exists.
                let existing = backend.load(&effective_key);
                if !existing.is_empty() {
                    let mut guard = agent.lock().await;
                    guard.seed_history(&existing);
                }

                *key_guard = Some(effective_key.clone());
            }
            drop(key_guard);
        }
        drop(backend_guard);

        // Record history length before turn for diffing.
        let pre_turn_len = {
            let guard = agent.lock().await;
            guard.history().len()
        };

        let result = {
            let mut guard = agent.lock().await;
            guard
                .turn_streaming(&message, tx.clone(), cancel_token, observer)
                .await
        };
        if let Err(e) = result {
            let _ = tx
                .send(StreamEvent::Error {
                    message: e.to_string(),
                })
                .await;
        }

        // Post-turn persistence: append new messages to session backend.
        let backend_guard = session_backend.read().await;
        if let Some(ref backend) = *backend_guard {
            let guard = agent.lock().await;
            let history = guard.history();
            if history.len() > pre_turn_len {
                let new_entries = &history[pre_turn_len..];
                let chat_msgs = conversation_messages_to_chat_messages(new_entries);
                for msg in &chat_msgs {
                    if let Err(e) = backend.append(&effective_key, msg) {
                        tracing::warn!(
                            "Failed to persist message to session '{effective_key}': {e}"
                        );
                        let _ = tx
                            .send(StreamEvent::Error {
                                message: format!("session persistence failed: {e}"),
                            })
                            .await;
                        break;
                    }
                }
            }
        }

        // Post-turn TTS: synthesize audio from the last assistant message if TTS is enabled.
        let config = config_manager.get_config().await;
        if config.tts.enabled {
            // Extract assistant response text from the last history entry.
            let assistant_text = {
                let guard = agent.lock().await;
                let history = guard.history();
                history.last().and_then(|msg| match msg {
                    ConversationMessage::Chat(cm) if cm.role == "assistant" => {
                        Some(cm.content.clone())
                    }
                    _ => None,
                })
            };
            if let Some(text) = assistant_text {
                if let Ok(tts_manager) = crate::channels::tts::TtsManager::new(&config.tts) {
                    match tts_manager.synthesize(&text).await {
                        Ok(audio_bytes) => {
                            use base64::{engine::general_purpose::STANDARD, Engine as _};
                            let audio_b64 = STANDARD.encode(&audio_bytes);
                            let _ = tx
                                .send(StreamEvent::Audio {
                                    format: config.tts.default_format.clone(),
                                    data: audio_b64,
                                })
                                .await;
                        }
                        Err(e) => {
                            tracing::debug!("TTS synthesis skipped for API response: {e}");
                        }
                    }
                }
            }
        }
    });

    Ok(())
}

/// Rebuild the agent from the latest config if the config has changed.
///
/// Uses the `AgentHandle::has_config_changed()` persistent tracker (T029)
/// so that the agent is only rebuilt when `update_config()` or
/// `reload_config_from_file()` has been called since the last check.
async fn apply_config_changes_if_needed(
    agent: &std::sync::Arc<tokio::sync::Mutex<Agent>>,
    config_manager: &std::sync::Arc<crate::api::config::RuntimeConfigManager>,
    config_rx: &tokio::sync::Mutex<tokio::sync::watch::Receiver<crate::config::Config>>,
    host_tool_registry: &std::sync::Arc<crate::api::host_tools::HostToolRegistry>,
    cancel_token: &tokio_util::sync::CancellationToken,
    session_backend: &std::sync::Arc<
        tokio::sync::RwLock<
            Option<std::sync::Arc<dyn crate::channels::session_backend::SessionBackend>>,
        >,
    >,
    current_session_key: &std::sync::Arc<tokio::sync::Mutex<Option<String>>>,
) -> Result<(), crate::api::types::ApiError> {
    let changed = {
        let rx = config_rx.lock().await;
        rx.has_changed().unwrap_or(false)
    };
    if !changed {
        return Ok(());
    }

    // Capture old workspace_dir before rebuild.
    let old_config = config_manager.get_config().await;
    let old_workspace = old_config.workspace_dir.clone();

    // Mark as seen before rebuilding so concurrent calls don't double-rebuild.
    {
        let mut rx = config_rx.lock().await;
        rx.mark_changed();
    }
    let config = config_manager.get_config().await;

    // Attempt agent rebuild — log but don't abort if it fails (e.g. mock providers in tests).
    match Agent::from_config(&config).await {
        Ok(new_agent) => {
            let mut guard = agent.lock().await;
            *guard = new_agent;

            // T037: Re-inject host tools after agent rebuild (FR-008)
            let proxies = host_tool_registry.create_proxies(Some(cancel_token.clone()));
            if !proxies.is_empty() {
                guard.replace_host_tools(proxies);
            }
        }
        Err(e) => {
            tracing::warn!("Failed to rebuild agent from updated config: {e}");
        }
    }

    // Re-initialize session backend if workspace_dir changed.
    if config.workspace_dir != old_workspace && !config.workspace_dir.as_os_str().is_empty() {
        let new_backend: Option<
            std::sync::Arc<dyn crate::channels::session_backend::SessionBackend>,
        > = if config.channels_config.session_persistence {
            let backend_type = &config.channels_config.session_backend;
            if backend_type == "sqlite" {
                match crate::channels::session_sqlite::SqliteSessionBackend::new(
                    &config.workspace_dir,
                ) {
                    Ok(b) => Some(std::sync::Arc::new(b)),
                    Err(e) => {
                        tracing::warn!("Failed to re-initialize SQLite session backend: {e}");
                        None
                    }
                }
            } else {
                match crate::channels::session_store::SessionStore::new(&config.workspace_dir) {
                    Ok(b) => Some(std::sync::Arc::new(b)),
                    Err(e) => {
                        tracing::warn!("Failed to re-initialize JSONL session backend: {e}");
                        None
                    }
                }
            }
        } else {
            None
        };

        let mut backend_guard = session_backend.write().await;
        *backend_guard = new_backend;

        // Reset session key so next send_message reloads.
        let mut key_guard = current_session_key.lock().await;
        *key_guard = None;
    }

    Ok(())
}

/// Cancel any in-flight message processing for this handle.
///
/// If no message is being processed, this is a no-op.
pub fn cancel_message(handle: &AgentHandle) -> Result<(), ApiError> {
    if !handle.is_initialized() {
        return Err(ApiError::NotInitialized);
    }
    handle.cancel_and_reset();
    Ok(())
}

// ── Session Management ───────────────────────────────────────────

/// List all persisted sessions, sorted by last activity (newest first).
pub async fn list_sessions(
    handle: &AgentHandle,
) -> Result<Vec<crate::api::types::SessionInfo>, ApiError> {
    if !handle.is_initialized() {
        return Err(ApiError::NotInitialized);
    }
    let backend_arc = handle.session_backend();
    let backend_guard = backend_arc.read().await;
    let backend = backend_guard.as_ref().ok_or(ApiError::Internal {
        message: "no session backend available".into(),
    })?;

    let metadata = backend.list_sessions_with_metadata();
    let mut sessions: Vec<crate::api::types::SessionInfo> = metadata
        .into_iter()
        .map(|m| crate::api::types::SessionInfo {
            key: m.key,
            message_count: m.message_count,
            created_at: m.created_at.to_rfc3339(),
            last_activity: m.last_activity.to_rfc3339(),
            workspace_dir: m.workspace_dir,
        })
        .collect();

    // Sort by last_activity descending.
    sessions.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
    Ok(sessions)
}

/// Load the full message history for a session.
///
/// Returns an empty vec if the session does not exist.
pub async fn load_session_history(
    handle: &AgentHandle,
    session_key: String,
) -> Result<Vec<ChatMessage>, ApiError> {
    if !handle.is_initialized() {
        return Err(ApiError::NotInitialized);
    }
    crate::api::types::validate_session_key(&session_key)?;

    let backend_arc = handle.session_backend();
    let backend_guard = backend_arc.read().await;
    let backend = backend_guard.as_ref().ok_or(ApiError::Internal {
        message: "no session backend available".into(),
    })?;

    Ok(backend.load(&session_key))
}

/// Delete all persisted data for a session.
pub async fn delete_session(handle: &AgentHandle, session_key: String) -> Result<(), ApiError> {
    if !handle.is_initialized() {
        return Err(ApiError::NotInitialized);
    }
    crate::api::types::validate_session_key(&session_key)?;

    let backend_arc = handle.session_backend();
    let backend_guard = backend_arc.read().await;
    let backend = backend_guard.as_ref().ok_or(ApiError::Internal {
        message: "no session backend available".into(),
    })?;

    backend
        .delete_session(&session_key)
        .map_err(|e| ApiError::Internal {
            message: format!("failed to delete session '{session_key}': {e}"),
        })?;
    Ok(())
}

// ── FRB StreamSink wrappers ──────────────────────────────────────

/// FRB-compatible streaming send: accepts a `StreamSink<StreamEvent>` that
/// FRB translates into a Dart `Stream<StreamEvent>` on the Flutter side.
///
/// Internally bridges to `send_message()` via an `mpsc` channel, forwarding
/// each event into the sink. The sink is closed after a terminal event
/// (`Done` or `Error`).
#[cfg(feature = "frb")]
pub fn send_message_stream(
    handle: &AgentHandle,
    message: String,
    session_key: Option<String>,
    sink: StreamSink<StreamEvent>,
) -> Result<(), ApiError> {
    let (tx, mut rx) = mpsc::channel::<StreamEvent>(64);
    send_message(handle, message, session_key, None, tx)?;

    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let is_terminal = matches!(event, StreamEvent::Done { .. } | StreamEvent::Error { .. });
            if sink.add(event).is_err() {
                break;
            }
            if is_terminal {
                break;
            }
        }
    });

    Ok(())
}
