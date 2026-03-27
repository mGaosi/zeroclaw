# Research: API Workspace Directory & Session Persistence

**Feature**: 004-api-workspace-session-persist
**Date**: 2026-03-26

## R1: Workspace Directory Injection into `init()`

**Question**: How should `workspace_dir` be passed to the API without breaking the existing `init(config_path, overrides)` signature?

**Decision**: Add `workspace_dir: Option<String>` as a new field on `ConfigPatch`. Apply it in `ConfigPatch::apply_to()` to set `config.workspace_dir`. This piggybacks on the existing `overrides` parameter — no new `init()` parameter needed.

**Rationale**: `ConfigPatch` is the designated extensible configuration mechanism. Adding a field to it is a minimal, backward-compatible change. The `init()` signature stays stable for FRB codegen. `workspace_dir` can also be updated at runtime via `update_config()` using the same `ConfigPatch` field.

**Alternatives considered**:
- *New `init()` parameter*: Rejected — breaks the FRB-generated Dart bridge. Requires regenerating Flutter bindings and updating all call sites.
- *Separate `set_workspace_dir()` on `AgentHandle`*: Rejected — workspace must be set *before* agent construction (memory, tools, identity all depend on it). A post-init setter is too late.
- *Environment variable*: Rejected — not controllable from Dart/SDK, and doesn't work well on Android sandboxed apps.

## R2: Session Backend Placement in the API Layer

**Question**: Where should the session backend (`SessionBackend`) live in the API architecture?

**Decision**: Add `session_backend: Option<Arc<dyn SessionBackend>>` to `AgentHandle`. Initialize it during `init()` based on `config.channels_config.session_persistence` and `config.channels_config.session_backend`. The backend is shared via `Arc` and passed into the `send_message()` flow.

**Rationale**: `AgentHandle` already holds all per-instance state (`agent`, `config_manager`, `observer_registry`, etc.). Adding the session backend here follows the established pattern. Using `Arc<dyn SessionBackend>` keeps it thread-safe and polymorphic (SQLite or JSONL).

**Alternatives considered**:
- *Store in `Agent` struct*: Rejected — `Agent` is the LLM orchestration layer. Session persistence is a transport/API concern, not an agent concern. Also, Agent is rebuilt on config changes, which would lose the backend reference.
- *Store in `RuntimeConfigManager`*: Rejected — config manager handles configuration, not runtime services. Mixing concerns would violate single-responsibility.
- *Create a new `SessionManager` wrapper*: Rejected — over-engineering for what is a single `Arc<dyn SessionBackend>`.

## R3: Session Key in `send_message()`

**Question**: How should the session key be passed without breaking the existing `send_message(handle, message, tx)` signature?

**Decision**: Add `session_key: Option<String>` parameter to `send_message()`. When `None`, defaults to `"api_default"`. This is a breaking API change, but the API is not yet stabilized (no semver guarantee on the FRB bridge).

**Rationale**: The session key must be per-call to support multi-conversation apps. A new parameter is the cleanest approach — it's explicit, type-safe, and self-documenting. The FRB bridge regeneration handles the Dart side automatically.

**Alternatives considered**:
- *Session key on `AgentHandle`*: Rejected — would limit to one session per handle, defeating multi-conversation support.
- *Session key in `ConfigPatch`*: Rejected — session key is per-message, not per-config. Mixing them would require a config update before every message in multi-session apps.
- *Separate `set_active_session()` method*: Rejected — introduces mutable state and race conditions in concurrent multi-session usage.

## R4: Session History Auto-Load with Session-Key Tracking

**Question**: How should session history be loaded when a session key with existing persisted data is used?

**Decision**: Track the active session key on `AgentHandle` via `current_session_key: Arc<Mutex<Option<String>>>`. In the `send_message()` async task, before calling `agent.turn_streaming()`:
1. Resolve effective key: `session_key.unwrap_or("api_default")`.
2. Compare effective key against `current_session_key`.
3. If the key **differs** from the current session (or current is `None`):
   a. If a previous session was active, persist its current in-memory history to the backend.
   b. Call `agent.clear_history()` to reset in-memory state.
   c. If the backend has persisted data for the new key, call `backend.load(key)` → `agent.seed_history(&messages)`.
   d. Update `current_session_key` to the new key.
4. After `turn_streaming()` completes, append the new messages (user + assistant + tools) to the backend via `backend.append()`.

**Rationale**: A single `Agent` holds one linear history. Multi-session support requires clearing and reloading when the caller switches session keys. The `current_session_key` tracker makes this explicit and avoids the broken "check if history is empty" heuristic (which fails when switching from one non-empty session to another). Uses the existing `seed_history()` method which correctly builds the system prompt. The append-after-turn pattern is identical to how channels handle persistence.

**Alternatives considered**:
- *Check if history is empty*: Rejected — fails for multi-session: after sending to `chat_1`, history is non-empty, so `chat_2`'s persisted history would never load. History from `chat_1` would bleed into `chat_2`.
- *Load during `init()`*: Rejected — would pre-load all sessions or require specifying a session at init time.
- *Maintain a per-session agent cache*: Rejected — massively increases memory usage and complexity.

## R5: Workspace Directory Validation

**Question**: What validation should be performed on the workspace directory at `init()` time?

**Decision**: After `ConfigPatch::apply_to()` sets `config.workspace_dir`, but before `Agent::from_config()`:
1. Call `tokio::fs::create_dir_all(&config.workspace_dir)` to ensure it exists.
2. Attempt to create and remove a temp file to verify write permissions.
3. On failure, return `ApiError::ValidationError` with a descriptive message.

**Rationale**: Failing fast at init prevents cryptic errors later when the agent tries to write memory, sessions, or workspace files. `create_dir_all` is idempotent and handles the "non-existent path" case. The write test handles the "read-only filesystem" case.

**Alternatives considered**:
- *Defer validation to first write*: Rejected — spec requires SC-006 (catch 100% of unwritable paths at init).
- *Check directory metadata only*: Rejected — directory might exist but not be writable (e.g., wrong permissions, full disk).

## R6: Session Persistence Error Handling

**Question**: How should I/O errors during session persistence be surfaced without blocking the conversation?

**Decision**: Session persistence runs *after* `turn_streaming()` completes. If `append()` fails:
1. Log at `warn!` level with the error details, session key, and workspace path.
2. Send a `StreamEvent::Error` event with a user-friendly message.
3. Do NOT abort the conversation or discard the in-memory history.

**Rationale**: The spec (FR-014) explicitly requires persistence to be non-blocking. Users should be informed of the failure but not lose their conversation. The in-memory history remains intact for the current session.

**Alternatives considered**:
- *Retry with backoff*: Rejected (for now) — adds complexity. If disk is full, retries won't help. Can be added later if transient errors are observed.
- *Silent logging only*: Rejected — spec requires the caller to be notified via stream event.
- *Async background persistence*: Rejected — `append()` on SQLite WAL is fast enough (<1ms). Background persistence adds ordering complexity for sequential messages.

## R7: File Permissions for Session Storage

**Question**: How to enforce 0600 permissions on session files?

**Decision**: The `SqliteSessionBackend::new()` already creates the database file via rusqlite. After creation, call `std::fs::set_permissions()` with mode 0600. For JSONL, set permissions on the `.jsonl` file when first created in `SessionStore::append()`.

**Rationale**: Unix file permissions (0600 = owner read/write only) are the standard mechanism for protecting sensitive files. The existing backends don't set explicit permissions (they inherit the umask default), so this is a targeted addition.

**Alternatives considered**:
- *Set umask before file operations*: Rejected — umask is process-global, would affect all file creation including tool operations.
- *Use OS-specific ACLs*: Rejected — adds platform-specific complexity for marginal benefit over basic permissions.
- *Skip on non-Unix platforms*: Accepted — Android/iOS have their own app sandboxing; explicit permissions are a defense-in-depth layer on desktop/server Unix.

## R8: Session Management API Functions

**Question**: What session management functions should be exposed?

**Decision**: Add three public functions to `src/api/conversation.rs`:
- `list_sessions(handle) -> Result<Vec<SessionInfo>, ApiError>` — returns session metadata
- `load_session_history(handle, session_key) -> Result<Vec<ChatMessage>, ApiError>` — returns full message history
- `delete_session(handle, session_key) -> Result<(), ApiError>` — removes session from backend

Where `SessionInfo` is a new API type (simplified from `SessionMetadata`) containing `key`, `created_at`, `last_activity`, `message_count`.

**Rationale**: These three operations cover the essential CRUD surface for session management (Create is implicit via `send_message`). The function signatures follow the existing API pattern (taking `&AgentHandle`, returning `Result<_, ApiError>`).

**Alternatives considered**:
- *Full CRUD with rename/search*: Rejected for now — spec explicitly puts search out of scope. Can be added later.
- *Methods on `AgentHandle`*: Considered but rejected — other API functions (`send_message`, `cancel_message`) are free functions, not methods. Consistency wins.

## R9: History Tracking for Persistence

**Question**: How to capture all messages (including tool calls/results) for session persistence, given that `SessionBackend::append()` takes `ChatMessage` (not `ConversationMessage`)?

**Decision**: After each `turn_streaming()` call, diff the agent's `history` to find new entries. Convert each `ConversationMessage` variant to one or more `ChatMessage` records for persistence:
- `Chat(m)` → `m` as-is
- `AssistantToolCalls { text, tool_calls, .. }` → `ChatMessage::assistant(serialized)` with tool_calls JSON-encoded in content
- `ToolResults(results)` → `ChatMessage::tool(serialized)` per result

**Rationale**: The existing `SessionBackend` trait speaks `ChatMessage`. Rather than modifying the trait (which would break channel mode), we adapt `ConversationMessage` → `ChatMessage` at the API layer boundary. This is the same pattern channels already use for serializing their richer message types.

**Alternatives considered**:
- *Extend `SessionBackend` to accept `ConversationMessage`*: Rejected — breaks the existing trait contract and all implementations. High risk, high churn.
- *Store JSON blobs instead of ChatMessage*: Rejected — loses compatibility with the existing session format and FTS5 indexing.
- *Only persist user and final assistant text*: Rejected — spec requires "complete conversations including tool interactions" (FR-004).
