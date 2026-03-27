# API Contract: Workspace Directory & Session Persistence

**Feature**: 004-api-workspace-session-persist
**Date**: 2026-03-26
**Type**: Rust public API (consumed via Flutter Rust Bridge)

## Public API Functions

### `init()` — Agent Initialization

```
init(config_path: Option<String>, overrides: Option<ConfigPatch>) -> Result<AgentHandle, ApiError>
```

**Changed behavior**: When `overrides.workspace_dir` is `Some(path)`:
1. Sets `config.workspace_dir` to the provided path
2. Validates the path is writable (creating directories if needed)
3. Returns `ApiError::ValidationError` if the path is invalid or unwritable
4. Initializes session backend in the workspace directory

**Backward compatible**: Existing callers passing `None` or a `ConfigPatch` without `workspace_dir` see no behavior change.

---

### `send_message()` — Streaming Conversation

```
send_message(handle: &AgentHandle, message: String, session_key: Option<String>, tx: Sender<StreamEvent>) -> Result<(), ApiError>
```

**New parameter**: `session_key: Option<String>`
- When `None` → uses default key `"api_default"`
- When `Some(key)` → uses the provided key (validated: alphanumeric, `_`, `-` only)

**New behavior** (when session backend is available):
1. Tracks active session key via `handle.current_session_key`. On session key change: persists current history, clears agent history, loads new session via `seed_history()`.
2. After turn completes, appends all new messages to the session backend.
3. On persistence failure, sends `StreamEvent::Error` and logs at `warn` level.

**Breaking change**: New `session_key` parameter. FRB bridge must be regenerated.

---

### `send_message_stream()` — FRB StreamSink Wrapper

```
send_message_stream(handle: &AgentHandle, message: String, session_key: Option<String>, sink: StreamSink<StreamEvent>) -> Result<(), ApiError>
```

Same as `send_message()` but bridges to FRB `StreamSink`. Updated to pass through `session_key`.

---

### `list_sessions()` — List Persisted Sessions (NEW)

```
list_sessions(handle: &AgentHandle) -> Result<Vec<SessionInfo>, ApiError>
```

**Returns**: List of session metadata, sorted by `last_activity` descending.
**Error cases**:
- `ApiError::NotInitialized` — if handle not initialized
- `ApiError::Internal` — if no session backend available

---

### `load_session_history()` — Load Session Messages (NEW)

```
load_session_history(handle: &AgentHandle, session_key: String) -> Result<Vec<ChatMessage>, ApiError>
```

**Returns**: Full ordered message history for the given session key. Empty vec if session doesn't exist.
**Error cases**:
- `ApiError::NotInitialized` — if handle not initialized
- `ApiError::ValidationError` — if session_key is empty

---

### `delete_session()` — Delete a Session (NEW)

```
delete_session(handle: &AgentHandle, session_key: String) -> Result<(), ApiError>
```

**Behavior**: Removes all persisted data for the given session key at the backend level.
**Error cases**:
- `ApiError::NotInitialized` — if handle not initialized
- `ApiError::ValidationError` — if session_key is empty
- `ApiError::Internal` — if deletion fails

---

### `cancel_message()` — Cancel In-Flight Processing (UNCHANGED)

```
cancel_message(handle: &AgentHandle) -> Result<(), ApiError>
```

No changes to this function.

---

## Types

### ConfigPatch (extended)

```
ConfigPatch {
    provider: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
    temperature: Option<f64>,
    system_prompt: Option<String>,
    max_tool_iterations: Option<usize>,
    max_history_messages: Option<usize>,
    workspace_dir: Option<String>,         // NEW
}
```

**Validation**: If `workspace_dir` is `Some`:
- Must be non-empty
- Must be a valid filesystem path
- At `init()` time: must be writable (or creatable)

---

### SessionInfo (NEW)

```
SessionInfo {
    key: String,
    message_count: usize,
    created_at: String,      // ISO 8601
    last_activity: String,   // ISO 8601
}
```

**Serialization**: `Serialize + Deserialize` for FRB bridge compatibility.

---

### StreamEvent (UNCHANGED)

No changes to `StreamEvent` variants. Persistence errors use the existing `Error { message }` variant.

---

### ApiError (UNCHANGED)

No new variants needed. Existing variants cover all error cases:
- `ValidationError` — for invalid workspace paths and session keys
- `NotInitialized` — for operations on uninitialized handles
- `Internal` — for backend failures
- `ConfigFileError` — for config I/O failures

---

## Module Exports

New public exports from `src/api/mod.rs`:

```
pub use conversation::{list_sessions, load_session_history, delete_session};
pub use types::SessionInfo;

#[cfg(feature = "frb")]
pub use conversation::send_message_stream;  // (already exported, signature changes)
```
