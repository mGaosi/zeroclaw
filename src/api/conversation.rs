use crate::agent::agent::Agent;
use crate::api::lifecycle::AgentHandle;
use crate::api::types::{ApiError, StreamEvent};
use tokio::sync::mpsc;

#[cfg(feature = "frb")]
use flutter_rust_bridge::StreamSink;

/// Send a message to the agent and receive streaming events.
///
/// Spawns an async task that pushes events into the returned receiver.
/// The function returns a receiver immediately; events arrive as they are
/// produced. The stream closes after a `Done` or `Error` event.
///
/// Before each turn, checks for config changes and rebuilds the agent's
/// provider if necessary (T029).
pub fn send_message(
    handle: &AgentHandle,
    message: String,
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

    let agent = handle.agent();
    let cancel_token = handle.cancel_token();
    let observer = handle.observer_registry();
    let config_manager = handle.config_manager();
    let config_rx = handle.config_rx();
    let host_tool_registry = handle.host_tool_registry();

    tokio::spawn(async move {
        // T029: Check for config changes and rebuild agent if needed
        if let Err(e) = apply_config_changes_if_needed(
            &agent,
            &config_manager,
            &config_rx,
            &host_tool_registry,
            &cancel_token,
        )
        .await
        {
            tracing::warn!("Failed to apply config changes: {e}");
        }

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
) -> Result<(), crate::api::types::ApiError> {
    let changed = {
        let rx = config_rx.lock().await;
        rx.has_changed().unwrap_or(false)
    };
    if !changed {
        return Ok(());
    }
    // Mark as seen before rebuilding so concurrent calls don't double-rebuild.
    {
        let mut rx = config_rx.lock().await;
        rx.mark_changed();
    }
    let config = config_manager.get_config().await;
    let new_agent = Agent::from_config(&config)
        .await
        .map_err(|e| ApiError::Internal {
            message: format!("failed to rebuild agent from updated config: {e}"),
        })?;
    let mut guard = agent.lock().await;
    *guard = new_agent;

    // T037: Re-inject host tools after agent rebuild (FR-008)
    let proxies = host_tool_registry.create_proxies(Some(cancel_token.clone()));
    if !proxies.is_empty() {
        guard.replace_host_tools(proxies);
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
    sink: StreamSink<StreamEvent>,
) -> Result<(), ApiError> {
    let (tx, mut rx) = mpsc::channel::<StreamEvent>(64);
    send_message(handle, message, tx)?;

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
