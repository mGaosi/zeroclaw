# Library API Contract: ZeroClaw Mobile SDK

**Feature**: [../spec.md](../spec.md)
**Data Model**: [../data-model.md](../data-model.md)

This document defines the public Rust API surface that flutter_rust_bridge (FRB) consumes for code generation. These are the functions and types exposed in `src/api/` that FRB translates to Dart.

---

## Module: `src/api/mod.rs`

Re-exports all public API types. FRB scans this module for code generation.

---

## Module: `src/api/conversation.rs` — Conversation Streaming

### `send_message`

Sends a user message and streams response events to the host.

```rust
/// Send a message to the agent and receive streaming events.
///
/// FRB translates the `StreamSink<StreamEvent>` into a Dart `Stream<StreamEvent>`.
/// The function spawns an async task that pushes events into the sink,
/// then returns immediately. The stream closes after a `Done` or `Error` event.
///
/// # Cancellation
/// Dropping the Dart Stream cancels the underlying Rust task via the sink's
/// closed signal. In-flight provider calls are aborted via tokio CancellationToken.
pub fn send_message(
    handle: &AgentHandle,
    message: String,
    sink: StreamSink<StreamEvent>,
) -> Result<(), ApiError>
```

**Preconditions**:
- `handle` is a valid, initialized `AgentHandle`
- `message` is a non-empty UTF-8 string

**Postconditions**:
- Events are pushed to `sink` in order: `Chunk*`, optionally interleaved with `ToolCall`/`ToolResult` pairs, ending with `Done` or `Error`
- The function returns `Ok(())` after spawning; errors during processing are delivered as `StreamEvent::Error`

**Event ordering guarantees**:
1. Zero or more `Chunk` events (text deltas)
2. If tool calls occur: `ToolCall` → `ToolResult` pairs (may repeat for multi-tool turns)
3. After tool results, the loop re-enters step 1
4. Exactly one terminal event: `Done { full_response }` on success, or `Error { message }` on failure

---

### `cancel_message`

Cancels an in-flight message processing.

```rust
/// Cancel any in-flight message processing for this handle.
///
/// If no message is being processed, this is a no-op.
/// The stream will receive an `Error` event with a cancellation message.
pub fn cancel_message(handle: &AgentHandle) -> Result<(), ApiError>
```

---

## Module: `src/api/config.rs` — Runtime Configuration

### `get_config`

Returns the current configuration as a serialized snapshot.

```rust
/// Get the current runtime configuration.
///
/// Returns a JSON-serialized string of the full Config struct.
/// The host app can parse this to display current settings.
pub fn get_config(handle: &AgentHandle) -> Result<String, ApiError>
```

**Returns**: JSON string of the current `Config`. FRB delivers this as a Dart `String`.

---

### `update_config`

Applies a partial configuration update.

```rust
/// Apply a partial configuration update at runtime.
///
/// Only fields set in `patch` are changed. Unset fields retain current values.
/// The update is validated before application. If validation fails, the
/// previous config remains active and an error is returned.
///
/// Changes take effect for the next interaction. In-flight requests
/// complete with the previous configuration.
pub fn update_config(
    handle: &AgentHandle,
    patch: ConfigPatch,
) -> Result<(), ApiError>
```

**Preconditions**:
- At least one field in `patch` is `Some`

**Postconditions**:
- On success: config updated, subscribers notified, file saved (if path set)
- On failure: `ApiError::ValidationError` with field-level messages; config unchanged

**Validation errors** (returned as `ApiError::ValidationError`):
- `temperature` out of range `[0.0, 2.0]`
- `provider` not in known provider list
- `api_key` is empty string
- `max_tool_iterations` is zero

---

### `reload_config_from_file`

Triggers a reload from the TOML config file.

```rust
/// Reload configuration from the TOML file.
///
/// Re-reads the config file, validates, merges with current in-memory
/// config, and notifies subscribers. If the file doesn't exist or is
/// invalid, returns an error and keeps the current config.
pub fn reload_config_from_file(handle: &AgentHandle) -> Result<(), ApiError>
```

**Preconditions**:
- A config file path was provided at initialization

**Postconditions**:
- On success: config reloaded from file, validated, merged, subscribers notified
- On failure: `ApiError` with reason; config unchanged

---

## Module: `src/api/observer.rs` — Observability

### `register_observer`

Registers a callback to receive system-wide observability events.

```rust
/// Register an observer callback to receive system events.
///
/// FRB translates the `StreamSink<ObserverEventDto>` into a Dart Stream.
/// Events are delivered asynchronously as they occur.
///
/// Returns an observer ID that can be used to unregister later.
pub fn register_observer(
    handle: &AgentHandle,
    sink: StreamSink<ObserverEventDto>,
) -> Result<u64, ApiError>
```

---

### `unregister_observer`

Removes a previously registered observer callback.

```rust
/// Unregister an observer callback.
///
/// The callback stops receiving events immediately.
/// If the ID is not found, this is a no-op.
pub fn unregister_observer(
    handle: &AgentHandle,
    observer_id: u64,
) -> Result<(), ApiError>
```

---

## Module: `src/api/lifecycle.rs` — Initialization & Shutdown

### `init`

Creates and initializes a ZeroClaw agent instance.

```rust
/// Initialize a ZeroClaw agent instance.
///
/// Loads config from the provided path (optional), applies any runtime
/// overrides from `overrides`, and starts the agent runtime.
///
/// # Arguments
/// - `config_path`: Optional path to a TOML config file. Pass `None` to
///   use built-in defaults (typical on Android where config is injected).
/// - `overrides`: Optional ConfigPatch applied after file loading. Use this
///   to inject secrets (API keys) at runtime.
///
/// # Returns
/// An `AgentHandle` that the host app uses for all subsequent operations.
pub fn init(
    config_path: Option<String>,
    overrides: Option<ConfigPatch>,
) -> Result<AgentHandle, ApiError>
```

**Preconditions**:
- If `config_path` is `Some`, the path must be readable (or the call returns an error)
- If `overrides` contains an `api_key`, it must be non-empty

**Postconditions**:
- Agent runtime is initialized and ready to accept messages
- No network ports are bound (unless gateway feature is enabled and started separately)

---

### `shutdown`

Gracefully shuts down the agent instance.

```rust
/// Gracefully shut down the ZeroClaw instance.
///
/// Cancels any in-flight requests, flushes pending observer events,
/// and releases all resources. The handle becomes invalid after this call.
pub fn shutdown(handle: AgentHandle) -> Result<(), ApiError>
```

---

## Error Type

```rust
/// Errors returned by the public API.
#[derive(Debug, Clone)]
pub enum ApiError {
    /// Configuration validation failed.
    ValidationError { message: String },
    /// The agent is not initialized or has been shut down.
    NotInitialized,
    /// An internal error occurred.
    Internal { message: String },
    /// The config file could not be read or parsed.
    ConfigFileError { message: String },
}
```

---

## FRB Code Generation Notes

1. **All public types** in `src/api/` must use `#[frb]` attributes where needed
2. **Enums with data** (StreamEvent, ObserverEventDto, ApiError) translate to Dart sealed classes
3. **StreamSink<T>** is FRB's mechanism for Rust→Dart streaming; on the Dart side it appears as `Stream<T>`
4. **Option<T>** translates to nullable Dart types (`T?`)
5. **Result<T, E>** translates to Dart exceptions (FRB-managed)
6. **String** is UTF-8 on both sides (no encoding issues)
7. **AgentHandle** is opaque to Dart — FRB manages the pointer lifecycle
