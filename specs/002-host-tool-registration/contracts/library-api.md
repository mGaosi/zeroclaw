# Library API Contract: Host Tool Registration

**Feature**: 002-host-tool-registration
**Date**: 2026-03-24

This document defines the public Rust API surface for host-side tool registration. These functions are consumed by Flutter apps via flutter_rust_bridge (FRB).

## Types

### HostToolSpec

```rust
/// Definition of a host-side tool provided at registration time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostToolSpec {
    pub name: String,
    pub description: String,
    /// JSON-encoded parameter schema (JSON Schema subset).
    pub parameters_schema: String,
    /// Per-tool timeout in seconds. Defaults to 30 if None.
    pub timeout_seconds: Option<u32>,
}
```

### ToolRequest

```rust
/// Sent from ZeroClaw to the host app when the LLM invokes a host tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    pub request_id: String,
    pub tool_name: String,
    /// JSON-encoded arguments from the LLM.
    pub arguments: String,
}
```

### ToolResponse

```rust
/// Sent from the host app to ZeroClaw with the tool execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    pub request_id: String,
    pub output: String,
    pub success: bool,
}
```

## Functions

### register_tool

Registers a host-side tool with ZeroClaw. The tool becomes available to the LLM in subsequent conversation turns.

```rust
pub fn register_tool(
    handle: &AgentHandle,
    spec: HostToolSpec,
) -> Result<u64, ApiError>
```

**Parameters**:
- `handle`: Initialized `AgentHandle` (FR-013 — must be initialized)
- `spec`: Tool definition with name, description, parameter schema

**Returns**: `u64` — registration ID for later unregistration

**Errors**:
- `ApiError::NotInitialized` — handle not initialized
- `ApiError::ValidationError` — name is empty, collides with built-in/existing tool (FR-009), or schema is invalid (FR-010)

**Thread safety**: Safe to call from any thread (FR-011). Internally uses `parking_lot::Mutex`.

**Behavior**:
- Tool is available from the next conversation turn (not mid-turn)
- Registration triggers agent tool list refresh

---

### unregister_tool

Removes a previously registered host tool.

```rust
pub fn unregister_tool(
    handle: &AgentHandle,
    tool_id: u64,
) -> Result<(), ApiError>
```

**Parameters**:
- `handle`: Initialized `AgentHandle`
- `tool_id`: Registration ID returned by `register_tool`

**Returns**: `()` on success

**Errors**:
- `ApiError::NotInitialized` — handle not initialized
- `ApiError::ValidationError` — `tool_id` not found

**Behavior**:
- In-flight invocations of this tool complete normally (edge case from spec)
- Removal takes effect from the next conversation turn
- Does NOT block the calling thread (FR-011)

---

### setup_tool_handler

Sets up the tool execution channel. Must be called before any host tool can be invoked. Can be called again to re-establish the channel after the previous one closes (FR-014).

```rust
pub fn setup_tool_handler(
    handle: &AgentHandle,
    sender: mpsc::UnboundedSender<ToolRequest>,
) -> Result<(), ApiError>
```

**Parameters**:
- `handle`: Initialized `AgentHandle`
- `sender`: Channel sender that will receive `ToolRequest` messages

**Returns**: `()` on success

**Errors**:
- `ApiError::NotInitialized` — handle not initialized

**Behavior**:
- **First call**: Takes the initial mpsc receiver and spawns a forwarding task
- **Subsequent calls** (FR-014): Calls `reset_channel()` internally — creates a fresh mpsc pair, swaps the shared sender (existing proxies automatically use the new sender), then takes the new receiver and spawns a new forwarding task. In-flight invocations on the old channel receive a "channel closed" error.
- This supports mobile app lifecycle recovery (e.g., re-establishing after app backgrounding)

**Note**: The Rust-native API uses an `mpsc::UnboundedSender`. The FRB wrapper (`setup_tool_handler_stream`) accepts a `StreamSink<ToolRequest>` instead.

---

### submit_tool_response

Returns the result of a host tool execution to ZeroClaw.

```rust
pub fn submit_tool_response(
    handle: &AgentHandle,
    response: ToolResponse,
) -> Result<(), ApiError>
```

**Parameters**:
- `handle`: Initialized `AgentHandle`
- `response`: Tool execution result with matching `request_id`

**Returns**: `()` on success

**Errors**:
- `ApiError::NotInitialized` — handle not initialized

**Behavior**:
- If `request_id` does not match any pending request (timed out or cancelled), the response is silently discarded with a debug log
- This is not an error — it's expected behavior for timeout/cancellation races
- Safe to call from any thread (FR-011)

---

## FRB Wrappers

These functions are available only with the `frb` feature flag (`#[cfg(feature = "frb")]`).

### setup_tool_handler_stream

```rust
#[cfg(feature = "frb")]
pub fn setup_tool_handler_stream(
    handle: &AgentHandle,
    sink: StreamSink<ToolRequest>,
) -> Result<(), ApiError>
```

Bridges the tool request channel to an FRB `StreamSink`, equivalent to `register_observer_stream` pattern. Internally creates an `mpsc::unbounded_channel`, calls `setup_tool_handler` with the sender, and spawns a task that forwards from the receiver to the sink.

---

## Module Exports (src/api/mod.rs)

```rust
pub mod host_tools;

// Re-exports
pub use host_tools::HostToolRegistry;
pub use types::{HostToolSpec, ToolRequest, ToolResponse};  // added to types.rs

// FRB wrappers
#[cfg(feature = "frb")]
pub use host_tools::setup_tool_handler_stream;
```

---

## FR Coverage

| Function | FRs Covered |
|----------|-------------|
| `register_tool` | FR-001, FR-003, FR-009, FR-010, FR-011, FR-013 |
| `unregister_tool` | FR-002, FR-011 |
| `setup_tool_handler` | FR-004, FR-005, FR-014 |
| `submit_tool_response` | FR-004, FR-006, FR-012 |
| `HostToolProxy::execute` | FR-004, FR-006, FR-007, FR-012 |
| `Agent::replace_host_tools` | FR-008 |
| `setup_tool_handler_stream` | FR-005 (FRB compat) |
