use crate::api::lifecycle::AgentHandle;
use crate::api::types::{ApiError, HostToolSpec, ToolRequest, ToolResponse};
use crate::tools::{Tool, ToolResult};
use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

const DEFAULT_TIMEOUT_SECONDS: u32 = 30;

/// Internal metadata for a registered host tool.
struct HostToolMeta {
    name: String,
    description: String,
    parameters_schema: serde_json::Value,
    timeout: Duration,
}

/// Runtime registry tracking all currently registered host tools and
/// managing the execution channel.
pub struct HostToolRegistry {
    request_tx: Arc<Mutex<mpsc::UnboundedSender<ToolRequest>>>,
    request_rx: Mutex<Option<mpsc::UnboundedReceiver<ToolRequest>>>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<ToolResponse>>>>,
    tools: Arc<Mutex<HashMap<u64, HostToolMeta>>>,
    next_id: Mutex<u64>,
}

impl HostToolRegistry {
    /// Create a new registry with an unbounded mpsc channel pair.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            request_tx: Arc::new(Mutex::new(tx)),
            request_rx: Mutex::new(Some(rx)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            tools: Arc::new(Mutex::new(HashMap::new())),
            next_id: Mutex::new(1),
        }
    }

    /// Register a host tool after validation. Returns the registration ID.
    ///
    /// `builtin_tool_names` is obtained from `Agent::tool_names()` at the
    /// call site and used for name collision checking.
    pub fn register(
        &self,
        spec: HostToolSpec,
        builtin_tool_names: &[String],
    ) -> Result<u64, ApiError> {
        validate_spec(&spec)?;

        let tools = self.tools.lock();

        // Check collision with built-in tools
        if builtin_tool_names.iter().any(|n| n == &spec.name) {
            return Err(ApiError::ValidationError {
                message: format!("tool name '{}' collides with a built-in tool", spec.name),
            });
        }

        // Check collision with existing host tools
        if tools.values().any(|m| m.name == spec.name) {
            return Err(ApiError::ValidationError {
                message: format!("host tool '{}' is already registered", spec.name),
            });
        }

        drop(tools);

        let parsed_schema: serde_json::Value = serde_json::from_str(&spec.parameters_schema)
            .map_err(|e| ApiError::ValidationError {
                message: format!("parameters_schema is not valid JSON: {e}"),
            })?;

        if !parsed_schema.is_object() {
            return Err(ApiError::ValidationError {
                message: "parameters_schema must be a JSON object".into(),
            });
        }

        let timeout = Duration::from_secs(u64::from(
            spec.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECONDS),
        ));

        let mut id_guard = self.next_id.lock();
        let id = *id_guard;
        *id_guard += 1;

        self.tools.lock().insert(
            id,
            HostToolMeta {
                name: spec.name,
                description: spec.description,
                parameters_schema: parsed_schema,
                timeout,
            },
        );

        Ok(id)
    }

    /// Unregister a tool by registration ID.
    pub fn unregister(&self, tool_id: u64) -> Result<(), ApiError> {
        let mut tools = self.tools.lock();
        if tools.remove(&tool_id).is_none() {
            return Err(ApiError::ValidationError {
                message: format!("tool_id {tool_id} not found"),
            });
        }
        Ok(())
    }

    /// Snapshot current tools and return `Vec<Box<dyn Tool>>` of proxy instances.
    pub fn create_proxies(
        &self,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
    ) -> Vec<Box<dyn Tool>> {
        let tools = self.tools.lock();
        tools
            .values()
            .map(|meta| {
                let proxy = HostToolProxy {
                    name: meta.name.clone(),
                    description: meta.description.clone(),
                    parameters_schema: meta.parameters_schema.clone(),
                    request_tx: Arc::clone(&self.request_tx),
                    pending: Arc::clone(&self.pending),
                    timeout: meta.timeout,
                    cancel_token: cancel_token.clone(),
                };
                Box::new(proxy) as Box<dyn Tool>
            })
            .collect()
    }

    /// Take the mpsc receiver for `setup_tool_handler`.
    ///
    /// On first call, returns the initial receiver created in `new()`.
    /// On subsequent calls, creates a fresh channel pair via `reset_channel()`
    /// and returns the new receiver (FR-014).
    pub(crate) fn take_receiver(&self) -> Result<mpsc::UnboundedReceiver<ToolRequest>, ApiError> {
        let mut rx_guard = self.request_rx.lock();
        if let Some(rx) = rx_guard.take() {
            return Ok(rx);
        }
        // Receiver already taken — re-establish channel (FR-014)
        drop(rx_guard);
        self.reset_channel();
        self.request_rx
            .lock()
            .take()
            .ok_or_else(|| ApiError::ValidationError {
                message: "failed to re-establish tool handler channel".into(),
            })
    }

    /// Create a fresh mpsc channel pair, swap the sender so existing proxies
    /// transparently use the new channel, and store the new receiver (FR-014).
    pub(crate) fn reset_channel(&self) {
        let (tx, rx) = mpsc::unbounded_channel();
        *self.request_tx.lock() = tx;
        *self.request_rx.lock() = Some(rx);
    }

    /// Access the pending map (for `submit_tool_response`).
    pub(crate) fn pending(&self) -> Arc<Mutex<HashMap<String, oneshot::Sender<ToolResponse>>>> {
        Arc::clone(&self.pending)
    }
}

/// Validate a `HostToolSpec` before registration.
fn validate_spec(spec: &HostToolSpec) -> Result<(), ApiError> {
    if spec.name.trim().is_empty() {
        return Err(ApiError::ValidationError {
            message: "tool name must not be empty".into(),
        });
    }
    if spec.description.trim().is_empty() {
        return Err(ApiError::ValidationError {
            message: "tool description must not be empty".into(),
        });
    }
    // Validate JSON parse-ability
    let parsed: serde_json::Value =
        serde_json::from_str(&spec.parameters_schema).map_err(|e| ApiError::ValidationError {
            message: format!("parameters_schema is not valid JSON: {e}"),
        })?;
    if !parsed.is_object() {
        return Err(ApiError::ValidationError {
            message: "parameters_schema must be a JSON object".into(),
        });
    }
    if let Some(timeout) = spec.timeout_seconds {
        if timeout == 0 {
            return Err(ApiError::ValidationError {
                message: "timeout_seconds must be > 0".into(),
            });
        }
    }
    Ok(())
}

/// Proxy that implements the `Tool` trait for a single host-registered tool.
/// Created by `HostToolRegistry::create_proxies()`.
pub struct HostToolProxy {
    name: String,
    description: String,
    parameters_schema: serde_json::Value,
    request_tx: Arc<Mutex<mpsc::UnboundedSender<ToolRequest>>>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<ToolResponse>>>>,
    timeout: Duration,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
}

#[async_trait]
impl Tool for HostToolProxy {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.parameters_schema.clone()
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let (resp_tx, resp_rx) = oneshot::channel::<ToolResponse>();

        // Insert the oneshot sender into the pending map
        self.pending.lock().insert(request_id.clone(), resp_tx);

        // Send the request to the host
        let request = ToolRequest {
            request_id: request_id.clone(),
            tool_name: self.name.clone(),
            arguments: args.to_string(),
        };

        if self.request_tx.lock().send(request).is_err() {
            self.pending.lock().remove(&request_id);
            anyhow::bail!("host tool channel closed");
        }

        // Wait for response with timeout and optional cancellation
        let result = if let Some(ref token) = self.cancel_token {
            tokio::select! {
                biased;
                () = token.cancelled() => {
                    self.pending.lock().remove(&request_id);
                    Err(anyhow::anyhow!("cancelled"))
                }
                res = tokio::time::timeout(self.timeout, resp_rx) => {
                    match res {
                        Ok(Ok(response)) => Ok(response),
                        Ok(Err(_)) => {
                            // oneshot sender dropped (registry shutdown)
                            Err(anyhow::anyhow!("host tool channel closed"))
                        }
                        Err(_) => {
                            self.pending.lock().remove(&request_id);
                            Err(anyhow::anyhow!(
                                "host tool '{}' timed out after {:?}",
                                self.name,
                                self.timeout
                            ))
                        }
                    }
                }
            }
        } else {
            match tokio::time::timeout(self.timeout, resp_rx).await {
                Ok(Ok(response)) => Ok(response),
                Ok(Err(_)) => Err(anyhow::anyhow!("host tool channel closed")),
                Err(_) => {
                    self.pending.lock().remove(&request_id);
                    Err(anyhow::anyhow!(
                        "host tool '{}' timed out after {:?}",
                        self.name,
                        self.timeout
                    ))
                }
            }
        };

        match result {
            Ok(response) => Ok(ToolResult {
                success: response.success,
                output: response.output,
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(e.to_string()),
            }),
        }
    }
}

// ── Public API Functions ──────────────────────────────────────────

/// Set up the tool execution channel. Must be called once before any
/// host tool can be invoked.
pub fn setup_tool_handler(
    handle: &AgentHandle,
    sender: mpsc::UnboundedSender<ToolRequest>,
) -> Result<(), ApiError> {
    if !handle.is_initialized() {
        return Err(ApiError::NotInitialized);
    }

    let registry = handle.host_tool_registry();
    let mut rx = registry.take_receiver()?;

    tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
            if sender.send(request).is_err() {
                break;
            }
        }
    });

    Ok(())
}

/// Submit a tool response from the host app back to ZeroClaw.
pub fn submit_tool_response(handle: &AgentHandle, response: ToolResponse) -> Result<(), ApiError> {
    if !handle.is_initialized() {
        return Err(ApiError::NotInitialized);
    }

    let registry = handle.host_tool_registry();
    let pending = registry.pending();
    let sender = pending.lock().remove(&response.request_id);

    if let Some(tx) = sender {
        // If the receiver was dropped (timeout/cancellation), silently discard
        let _ = tx.send(response);
    } else {
        tracing::debug!(
            "submit_tool_response: request_id '{}' not found (timed out or cancelled)",
            response.request_id
        );
    }

    Ok(())
}

/// Register a host-side tool with ZeroClaw.
pub fn register_tool(handle: &AgentHandle, spec: HostToolSpec) -> Result<u64, ApiError> {
    if !handle.is_initialized() {
        return Err(ApiError::NotInitialized);
    }

    let registry = handle.host_tool_registry();
    let agent = handle.agent();

    // Get built-in tool names from the agent (blocking on async lock via block_in_place)
    let builtin_names = {
        let guard = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(agent.lock())
        });
        guard.builtin_tool_names()
    };

    let id = registry.register(spec, &builtin_names)?;

    // Rebuild proxy list and inject into agent
    let cancel_token = handle.cancel_token();
    let proxies = registry.create_proxies(Some(cancel_token));
    {
        let mut guard = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(agent.lock())
        });
        guard.replace_host_tools(proxies);
    }

    Ok(id)
}

/// Unregister a previously registered host tool.
pub fn unregister_tool(handle: &AgentHandle, tool_id: u64) -> Result<(), ApiError> {
    if !handle.is_initialized() {
        return Err(ApiError::NotInitialized);
    }

    let registry = handle.host_tool_registry();
    registry.unregister(tool_id)?;

    // Rebuild proxy list and inject into agent
    let agent = handle.agent();
    let cancel_token = handle.cancel_token();
    let proxies = registry.create_proxies(Some(cancel_token));
    {
        let mut guard = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(agent.lock())
        });
        guard.replace_host_tools(proxies);
    }

    Ok(())
}

// ── FRB StreamSink wrapper ────────────────────────────────────────

/// FRB-compatible tool handler: accepts a `StreamSink<ToolRequest>` that FRB
/// translates into a Dart `Stream<ToolRequest>`.
///
/// Internally bridges to `setup_tool_handler()` via an `mpsc::unbounded_channel`,
/// forwarding tool requests from the channel into the sink.
#[cfg(feature = "frb")]
pub fn setup_tool_handler_stream(
    handle: &AgentHandle,
    sink: flutter_rust_bridge::StreamSink<ToolRequest>,
) -> Result<(), ApiError> {
    let (tx, mut rx) = mpsc::unbounded_channel::<ToolRequest>();
    setup_tool_handler(handle, tx)?;

    tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
            if sink.add(request).is_err() {
                break;
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_spec(name: &str) -> HostToolSpec {
        HostToolSpec {
            name: name.to_string(),
            description: "A test tool".to_string(),
            parameters_schema: r#"{"type":"object","properties":{}}"#.to_string(),
            timeout_seconds: None,
        }
    }

    // ── T014: HostToolSpec validation ──

    #[test]
    fn valid_spec_passes_validation() {
        assert!(validate_spec(&valid_spec("my_tool")).is_ok());
    }

    #[test]
    fn empty_name_rejected() {
        let spec = HostToolSpec {
            name: String::new(),
            ..valid_spec("x")
        };
        let err = validate_spec(&spec).unwrap_err();
        assert!(matches!(err, ApiError::ValidationError { .. }));
    }

    #[test]
    fn whitespace_only_name_rejected() {
        let spec = HostToolSpec {
            name: "   ".to_string(),
            ..valid_spec("x")
        };
        assert!(validate_spec(&spec).is_err());
    }

    #[test]
    fn empty_description_rejected() {
        let spec = HostToolSpec {
            description: String::new(),
            ..valid_spec("x")
        };
        assert!(validate_spec(&spec).is_err());
    }

    #[test]
    fn invalid_json_schema_rejected() {
        let spec = HostToolSpec {
            parameters_schema: "not json".to_string(),
            ..valid_spec("x")
        };
        assert!(validate_spec(&spec).is_err());
    }

    #[test]
    fn non_object_json_schema_rejected() {
        let spec = HostToolSpec {
            parameters_schema: r#"[1,2,3]"#.to_string(),
            ..valid_spec("x")
        };
        assert!(validate_spec(&spec).is_err());
    }

    #[test]
    fn zero_timeout_rejected() {
        let spec = HostToolSpec {
            timeout_seconds: Some(0),
            ..valid_spec("x")
        };
        assert!(validate_spec(&spec).is_err());
    }

    #[test]
    fn positive_timeout_accepted() {
        let spec = HostToolSpec {
            timeout_seconds: Some(60),
            ..valid_spec("x")
        };
        assert!(validate_spec(&spec).is_ok());
    }

    // ── T015: HostToolRegistry::register() ──

    #[test]
    fn register_returns_id() {
        let registry = HostToolRegistry::new();
        let id = registry.register(valid_spec("tool_a"), &[]).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn register_increments_ids() {
        let registry = HostToolRegistry::new();
        let id1 = registry.register(valid_spec("tool_a"), &[]).unwrap();
        let id2 = registry.register(valid_spec("tool_b"), &[]).unwrap();
        assert!(id2 > id1);
    }

    #[test]
    fn register_duplicate_host_tool_rejected() {
        let registry = HostToolRegistry::new();
        registry.register(valid_spec("tool_a"), &[]).unwrap();
        let err = registry.register(valid_spec("tool_a"), &[]).unwrap_err();
        assert!(matches!(err, ApiError::ValidationError { .. }));
    }

    #[test]
    fn register_builtin_collision_rejected() {
        let registry = HostToolRegistry::new();
        let builtins = vec!["shell".to_string(), "file_read".to_string()];
        let err = registry
            .register(valid_spec("shell"), &builtins)
            .unwrap_err();
        assert!(matches!(err, ApiError::ValidationError { .. }));
    }

    // ── T016: HostToolRegistry::unregister() ──

    #[test]
    fn unregister_success() {
        let registry = HostToolRegistry::new();
        let id = registry.register(valid_spec("tool_a"), &[]).unwrap();
        assert!(registry.unregister(id).is_ok());
    }

    #[test]
    fn unregister_unknown_id_error() {
        let registry = HostToolRegistry::new();
        let err = registry.unregister(999).unwrap_err();
        assert!(matches!(err, ApiError::ValidationError { .. }));
    }

    // ── T017: HostToolRegistry::create_proxies() ──

    #[test]
    fn create_proxies_returns_correct_count() {
        let registry = HostToolRegistry::new();
        registry.register(valid_spec("tool_a"), &[]).unwrap();
        registry.register(valid_spec("tool_b"), &[]).unwrap();
        let proxies = registry.create_proxies(None);
        assert_eq!(proxies.len(), 2);
    }

    #[test]
    fn create_proxies_have_correct_metadata() {
        let registry = HostToolRegistry::new();
        let spec = HostToolSpec {
            name: "lookup".to_string(),
            description: "Look up data".to_string(),
            parameters_schema: r#"{"type":"object","properties":{"q":{"type":"string"}}}"#
                .to_string(),
            timeout_seconds: Some(10),
        };
        registry.register(spec, &[]).unwrap();
        let proxies = registry.create_proxies(None);
        assert_eq!(proxies.len(), 1);
        assert_eq!(proxies[0].name(), "lookup");
        assert_eq!(proxies[0].description(), "Look up data");
        let schema = proxies[0].parameters_schema();
        assert_eq!(schema["properties"]["q"]["type"], "string");
    }

    // ── T018b: Concurrent register/unregister (FR-011) ──

    #[tokio::test]
    async fn concurrent_register_unregister_no_panic() {
        let registry = Arc::new(HostToolRegistry::new());
        let mut handles = Vec::new();

        for i in 0..10 {
            let reg = Arc::clone(&registry);
            handles.push(tokio::spawn(async move {
                let name = format!("concurrent_tool_{i}");
                reg.register(valid_spec(&name), &[]).ok()
            }));
        }

        let mut ids = Vec::new();
        for handle in handles {
            if let Some(id) = handle.await.unwrap() {
                ids.push(id);
            }
        }

        // All 10 registrations should succeed (unique names)
        assert_eq!(ids.len(), 10);

        // Now unregister concurrently
        let mut unreg_handles = Vec::new();
        for id in ids {
            let reg = Arc::clone(&registry);
            unreg_handles.push(tokio::spawn(async move { reg.unregister(id) }));
        }

        for handle in unreg_handles {
            assert!(handle.await.unwrap().is_ok());
        }

        // Registry should be empty
        let proxies = registry.create_proxies(None);
        assert_eq!(proxies.len(), 0);
    }

    // ── T024: HostToolProxy::execute() happy path ──

    #[tokio::test]
    async fn execute_happy_path_round_trip() {
        let registry = HostToolRegistry::new();
        registry.register(valid_spec("greet"), &[]).unwrap();

        let (handler_tx, mut handler_rx) = mpsc::unbounded_channel::<ToolRequest>();
        let mut rx = registry.take_receiver().unwrap();
        // Forward from registry to handler
        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                let _ = handler_tx.send(req);
            }
        });

        let pending = registry.pending();
        let proxies = registry.create_proxies(None);
        let proxy = &proxies[0];

        // Spawn a responder
        let pending_clone = Arc::clone(&pending);
        tokio::spawn(async move {
            let req = handler_rx.recv().await.unwrap();
            assert_eq!(req.tool_name, "greet");
            let response = ToolResponse {
                request_id: req.request_id,
                output: "Hello!".to_string(),
                success: true,
            };
            let tx = pending_clone.lock().remove(&response.request_id).unwrap();
            let _ = tx.send(response);
        });

        let result = proxy
            .execute(serde_json::json!({"name": "world"}))
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.output, "Hello!");
    }

    // ── T025: HostToolProxy::execute() timeout ──

    #[tokio::test]
    async fn execute_timeout_returns_error() {
        let registry = HostToolRegistry::new();
        let spec = HostToolSpec {
            timeout_seconds: Some(1),
            ..valid_spec("slow_tool")
        };
        registry.register(spec, &[]).unwrap();

        // Take receiver but don't respond
        let _rx = registry.take_receiver().unwrap();
        let proxies = registry.create_proxies(None);
        let proxy = &proxies[0];

        let result = proxy.execute(serde_json::json!({})).await.unwrap();
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("timed out"));
    }

    // ── T026: submit_tool_response unknown request_id ──

    #[test]
    fn submit_unknown_request_id_silently_discarded() {
        let registry = HostToolRegistry::new();
        let pending = registry.pending();
        let response = ToolResponse {
            request_id: "nonexistent".to_string(),
            output: "data".to_string(),
            success: true,
        };
        // Manually test the pending map logic (we can't use the public fn
        // without an AgentHandle, so test the mechanism directly)
        let sender = pending.lock().remove(&response.request_id);
        assert!(sender.is_none()); // No panic, silently absent
    }

    // ── T027: setup_tool_handler second call — now succeeds via FR-014 ──

    #[test]
    fn take_receiver_second_call_succeeds_via_reset() {
        let registry = HostToolRegistry::new();
        let rx1 = registry.take_receiver();
        assert!(rx1.is_ok());
        // Second call triggers reset_channel() and returns a new receiver
        let rx2 = registry.take_receiver();
        assert!(rx2.is_ok());
    }

    // ── T028: register_tool not initialized ──
    // (This requires an AgentHandle, tested in integration test T029)

    // ── T030: Dynamic registration — register A, verify, register B, verify both,
    //          unregister A, verify only B ──

    #[test]
    fn dynamic_registration_lifecycle() {
        let registry = HostToolRegistry::new();
        let id_a = registry.register(valid_spec("tool_a"), &[]).unwrap();

        let names: Vec<String> = registry
            .create_proxies(None)
            .iter()
            .map(|p| p.name().to_string())
            .collect();
        assert!(names.contains(&"tool_a".into()));

        let _id_b = registry.register(valid_spec("tool_b"), &[]).unwrap();
        let names: Vec<String> = registry
            .create_proxies(None)
            .iter()
            .map(|p| p.name().to_string())
            .collect();
        assert!(names.contains(&"tool_a".into()));
        assert!(names.contains(&"tool_b".into()));

        registry.unregister(id_a).unwrap();
        let names: Vec<String> = registry
            .create_proxies(None)
            .iter()
            .map(|p| p.name().to_string())
            .collect();
        assert!(!names.contains(&"tool_a".into()));
        assert!(names.contains(&"tool_b".into()));
    }

    // ── T031: Unregister while in-flight ──

    #[tokio::test]
    async fn unregister_while_in_flight_completes() {
        let registry = Arc::new(HostToolRegistry::new());
        let id = registry.register(valid_spec("inflight_tool"), &[]).unwrap();

        let pending = registry.pending();

        // Set up receiver so send doesn't fail
        let (handler_tx, mut handler_rx) = mpsc::unbounded_channel::<ToolRequest>();
        let mut rx = registry.take_receiver().unwrap();
        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                let _ = handler_tx.send(req);
            }
        });

        // Create a proxy for the tool and spawn execute in background
        let proxies = registry.create_proxies(None);
        // Move the proxy into a spawned task
        let proxy_tool: Box<dyn Tool> = proxies.into_iter().next().unwrap();
        let pending_clone = Arc::clone(&pending);
        let execute_handle =
            tokio::spawn(async move { proxy_tool.execute(serde_json::json!({})).await });

        // Wait for the request to arrive
        let req = handler_rx.recv().await.unwrap();

        // Unregister the tool while execute is in-flight
        registry.unregister(id).unwrap();

        // Submit response — in-flight should complete normally
        let resp = ToolResponse {
            request_id: req.request_id.clone(),
            output: "completed".to_string(),
            success: true,
        };
        if let Some(tx) = pending_clone.lock().remove(&req.request_id) {
            let _ = tx.send(resp);
        }

        let result = execute_handle.await.unwrap().unwrap();
        assert!(result.success);
        assert_eq!(result.output, "completed");

        // Tool should no longer be in catalog
        let names: Vec<String> = registry
            .create_proxies(None)
            .iter()
            .map(|p| p.name().to_string())
            .collect();
        assert!(!names.contains(&"inflight_tool".into()));
    }

    // ── T032: Dynamic multi-tool lifecycle ──

    #[test]
    fn dynamic_multi_tool_lifecycle() {
        let registry = HostToolRegistry::new();
        let id_a = registry.register(valid_spec("alpha"), &[]).unwrap();
        let _id_b = registry.register(valid_spec("beta"), &[]).unwrap();

        let names: Vec<String> = registry
            .create_proxies(None)
            .iter()
            .map(|p| p.name().to_string())
            .collect();
        assert_eq!(names.len(), 2);

        registry.unregister(id_a).unwrap();

        let _id_c = registry.register(valid_spec("gamma"), &[]).unwrap();
        let mut names: Vec<String> = registry
            .create_proxies(None)
            .iter()
            .map(|p| p.name().to_string())
            .collect();
        names.sort();
        assert_eq!(names, vec!["beta", "gamma"]);
    }

    // ── T039: Rebuild persistence ──

    #[test]
    fn rebuild_persistence_tools_survive() {
        let registry = HostToolRegistry::new();
        registry.register(valid_spec("persist_a"), &[]).unwrap();
        registry.register(valid_spec("persist_b"), &[]).unwrap();

        // Simulate agent rebuild: create_proxies returns current tools
        let proxies = registry.create_proxies(None);
        assert_eq!(proxies.len(), 2);

        let names: Vec<String> = proxies.iter().map(|p| p.name().to_string()).collect();
        assert!(names.contains(&"persist_a".into()));
        assert!(names.contains(&"persist_b".into()));

        // Second call also returns them (simulating a second rebuild)
        let proxies2 = registry.create_proxies(None);
        assert_eq!(proxies2.len(), 2);
    }

    // ── T040: Cancellation ──

    #[tokio::test]
    async fn execute_cancelled_returns_promptly() {
        let registry = HostToolRegistry::new();
        registry.register(valid_spec("cancel_tool"), &[]).unwrap();

        let _rx = registry.take_receiver().unwrap();
        let cancel_token = tokio_util::sync::CancellationToken::new();
        let proxies = registry.create_proxies(Some(cancel_token.clone()));
        let proxy: Box<dyn Tool> = proxies.into_iter().next().unwrap();

        // Cancel immediately
        cancel_token.cancel();

        let start = std::time::Instant::now();
        let result = proxy.execute(serde_json::json!({})).await.unwrap();
        let elapsed = start.elapsed();

        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("cancelled"));
        // Should complete promptly (well before the 30s default timeout)
        assert!(elapsed < Duration::from_secs(2));
    }

    // ── T040b: Shutdown while pending ──

    #[tokio::test]
    async fn shutdown_while_pending_returns_channel_closed() {
        let registry = Arc::new(HostToolRegistry::new());
        registry.register(valid_spec("shutdown_tool"), &[]).unwrap();

        let _rx = registry.take_receiver().unwrap();
        let proxies = registry.create_proxies(None);
        let proxy: Box<dyn Tool> = proxies.into_iter().next().unwrap();

        // Spawn execute in background
        let exec_handle = tokio::spawn(async move { proxy.execute(serde_json::json!({})).await });

        // Give execute time to register pending request
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Drop the registry (simulating shutdown) — this drops the pending map
        // but the proxy has its own Arc clone, so we need to clear pending
        // to simulate. Actually dropping registry clears request_tx.
        // The oneshot sender is in the pending map which the proxy holds.
        // Let's clear the pending map to simulate shutdown.
        {
            let pending = registry.pending();
            let mut map = pending.lock();
            // Drop all senders — this causes oneshot receivers to get RecvError
            map.clear();
        }

        let result = exec_handle.await.unwrap().unwrap();
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("channel closed"));
    }

    // ── T041: Malformed response ──

    #[tokio::test]
    async fn malformed_response_returns_error_result() {
        let registry = HostToolRegistry::new();
        registry.register(valid_spec("err_tool"), &[]).unwrap();

        let pending = registry.pending();
        let (handler_tx, mut handler_rx) = mpsc::unbounded_channel::<ToolRequest>();
        let mut rx = registry.take_receiver().unwrap();
        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                let _ = handler_tx.send(req);
            }
        });

        let proxies = registry.create_proxies(None);
        let proxy: Box<dyn Tool> = proxies.into_iter().next().unwrap();

        let pending_clone = Arc::clone(&pending);
        let exec_handle = tokio::spawn(async move { proxy.execute(serde_json::json!({})).await });

        let req = handler_rx.recv().await.unwrap();

        // Submit failure response
        let resp = ToolResponse {
            request_id: req.request_id.clone(),
            output: String::new(),
            success: false,
        };
        if let Some(tx) = pending_clone.lock().remove(&req.request_id) {
            let _ = tx.send(resp);
        }

        let result = exec_handle.await.unwrap().unwrap();
        assert!(!result.success);
        assert_eq!(result.output, "");
    }

    // ── T045: setup_tool_handler can be called twice (FR-014) ──

    #[tokio::test]
    async fn setup_tool_handler_twice_new_channel_works() {
        let registry = HostToolRegistry::new();
        registry.register(valid_spec("re_tool"), &[]).unwrap();

        // First setup — take receiver and wire up a handler
        let mut rx1 = registry.take_receiver().unwrap();
        let (h1_tx, mut h1_rx) = mpsc::unbounded_channel::<ToolRequest>();
        tokio::spawn(async move {
            while let Some(req) = rx1.recv().await {
                let _ = h1_tx.send(req);
            }
        });

        // Verify first channel works
        let proxies1 = registry.create_proxies(None);
        let pending1 = registry.pending();
        let proxy1: Box<dyn Tool> = proxies1.into_iter().next().unwrap();
        let pending_c1 = Arc::clone(&pending1);
        let exec1 = tokio::spawn(async move { proxy1.execute(serde_json::json!({})).await });

        let req1 = h1_rx.recv().await.unwrap();
        if let Some(tx) = pending_c1.lock().remove(&req1.request_id) {
            let _ = tx.send(ToolResponse {
                request_id: req1.request_id,
                output: "from_channel_1".into(),
                success: true,
            });
        }
        let r1 = exec1.await.unwrap().unwrap();
        assert!(r1.success);
        assert_eq!(r1.output, "from_channel_1");

        // Second setup — reset channel, take new receiver
        let mut rx2 = registry.take_receiver().unwrap();
        let (h2_tx, mut h2_rx) = mpsc::unbounded_channel::<ToolRequest>();
        tokio::spawn(async move {
            while let Some(req) = rx2.recv().await {
                let _ = h2_tx.send(req);
            }
        });

        // Create new proxies and verify they use the new channel
        let proxies2 = registry.create_proxies(None);
        let pending2 = registry.pending();
        let proxy2: Box<dyn Tool> = proxies2.into_iter().next().unwrap();
        let pending_c2 = Arc::clone(&pending2);
        let exec2 = tokio::spawn(async move { proxy2.execute(serde_json::json!({})).await });

        let req2 = h2_rx.recv().await.unwrap();
        if let Some(tx) = pending_c2.lock().remove(&req2.request_id) {
            let _ = tx.send(ToolResponse {
                request_id: req2.request_id,
                output: "from_channel_2".into(),
                success: true,
            });
        }
        let r2 = exec2.await.unwrap().unwrap();
        assert!(r2.success);
        assert_eq!(r2.output, "from_channel_2");
    }

    // ── T046: In-flight on old channel fails after reset (FR-014) ──

    #[tokio::test]
    async fn in_flight_on_old_channel_fails_after_reset() {
        let registry = Arc::new(HostToolRegistry::new());
        registry.register(valid_spec("stale_tool"), &[]).unwrap();

        // First setup
        let rx1 = registry.take_receiver().unwrap();
        let proxies = registry.create_proxies(None);
        let proxy: Box<dyn Tool> = proxies.into_iter().next().unwrap();

        // Spawn execute — it sends through old channel
        let exec_handle = tokio::spawn(async move { proxy.execute(serde_json::json!({})).await });

        // Give execute time to send the request
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Drop old receiver (simulating handler gone) and reset channel
        drop(rx1);
        registry.reset_channel();

        // The in-flight invocation should fail with channel-closed
        // because the old receiver was dropped and the oneshot sender
        // in pending will never get a response. The timeout will fire.
        // But the old send() already went through before the drop.
        // Actually, once the old rx is dropped, the forwarding task
        // would have broken. The request was sent to the old channel;
        // no one will respond. Force-clear pending to simulate.
        {
            let pending = registry.pending();
            pending.lock().clear();
        }

        let result = exec_handle.await.unwrap().unwrap();
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("channel closed"));

        // Now verify new channel works
        let mut rx2 = registry.take_receiver().unwrap();
        let (h2_tx, mut h2_rx) = mpsc::unbounded_channel::<ToolRequest>();
        tokio::spawn(async move {
            while let Some(req) = rx2.recv().await {
                let _ = h2_tx.send(req);
            }
        });

        let proxies2 = registry.create_proxies(None);
        let pending2 = registry.pending();
        let proxy2: Box<dyn Tool> = proxies2.into_iter().next().unwrap();
        let pending_c2 = Arc::clone(&pending2);
        let exec2 = tokio::spawn(async move { proxy2.execute(serde_json::json!({})).await });

        let req2 = h2_rx.recv().await.unwrap();
        if let Some(tx) = pending_c2.lock().remove(&req2.request_id) {
            let _ = tx.send(ToolResponse {
                request_id: req2.request_id,
                output: "new_channel_ok".into(),
                success: true,
            });
        }
        let r2 = exec2.await.unwrap().unwrap();
        assert!(r2.success);
        assert_eq!(r2.output, "new_channel_ok");
    }

    // ── T047: Re-registration after channel reset (FR-014, FR-008) ──

    #[test]
    fn tools_persist_after_channel_reset() {
        let registry = HostToolRegistry::new();
        registry.register(valid_spec("persist_tool"), &[]).unwrap();

        // Take initial receiver
        let _rx1 = registry.take_receiver().unwrap();

        // Verify tool is registered
        let proxies1 = registry.create_proxies(None);
        assert_eq!(proxies1.len(), 1);
        assert_eq!(proxies1[0].name(), "persist_tool");

        // Reset channel (simulating second setup_tool_handler call)
        registry.reset_channel();

        // Tool should still be registered
        let proxies2 = registry.create_proxies(None);
        assert_eq!(proxies2.len(), 1);
        assert_eq!(proxies2[0].name(), "persist_tool");
    }
}

/// Compile-gate test: verifies `setup_tool_handler_stream` exists and has the
/// correct signature when the `frb` feature is enabled.
#[cfg(all(test, feature = "frb"))]
mod frb_tests {
    use super::*;

    #[test]
    fn setup_tool_handler_stream_signature() {
        // Verify the function exists and accepts the expected argument types.
        // We cannot construct a real StreamSink in tests, so we just verify
        // that the symbol resolves at compile time.
        let _fn_ptr: fn(
            &AgentHandle,
            flutter_rust_bridge::StreamSink<ToolRequest>,
        ) -> Result<(), ApiError> = setup_tool_handler_stream;
    }
}
