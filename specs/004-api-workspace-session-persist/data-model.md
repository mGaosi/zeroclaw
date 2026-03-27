# Data Model: API Workspace Directory & Session Persistence

**Feature**: 004-api-workspace-session-persist
**Date**: 2026-03-26

## Entities

### ConfigPatch (extended)

Partial configuration update for runtime changes. All fields optional — only `Some` values are applied.

| Field | Type | Validation | Description |
|-------|------|------------|-------------|
| provider | Option\<String\> | — | Provider name (existing) |
| model | Option\<String\> | — | Model identifier (existing) |
| api_key | Option\<String\> | non-empty | API key (existing) |
| api_base | Option\<String\> | — | API base URL (existing) |
| temperature | Option\<f64\> | 0.0–2.0 | Sampling temperature (existing) |
| system_prompt | Option\<String\> | — | System prompt override (existing) |
| max_tool_iterations | Option\<usize\> | > 0 | Max tool iterations (existing) |
| max_history_messages | Option\<usize\> | — | Max history messages (existing) |
| **workspace_dir** | **Option\<String\>** | **writable path** | **Workspace directory override (NEW)** |

**State transitions**: None — ConfigPatch is a stateless value object applied once.

### SessionInfo

Read-only metadata about a persisted conversation session.

| Field | Type | Description |
|-------|------|-------------|
| key | String | Unique session identifier (e.g., `"api_default"`, `"user_123_chat_5"`) |
| message_count | usize | Total messages in the session |
| created_at | String | ISO 8601 timestamp of session creation |
| last_activity | String | ISO 8601 timestamp of last message |

**Validation rules**: `key` must be non-empty and safe for filesystem paths (alphanumeric, `_`, `-`). **Note**: The existing `SessionMetadata` also has a `name` field; `SessionInfo` omits it because API-mode sessions are unnamed (session key serves as the identifier). If channel-mode sessions sharing the workspace have `name` set, it is accessible only via the backend directly, not through this API type.

**Relationships**: One `SessionInfo` ↔ one persisted session on disk. Listed via `list_sessions()`.

### AgentHandle (extended)

Opaque handle to a running agent instance. Holds all per-instance state.

| Field | Type | Description |
|-------|------|-------------|
| agent | Arc\<Mutex\<Agent\>\> | Agent instance (existing) |
| config_manager | Arc\<RuntimeConfigManager\> | Config management (existing) |
| observer_registry | Arc\<ObserverCallbackRegistry\> | Observer callbacks (existing) |
| host_tool_registry | Arc\<HostToolRegistry\> | Host tools (existing) |
| cancel_token | CancellationToken | Cancellation (existing) |
| config_rx | Arc\<Mutex\<watch::Receiver\>\> | Config change detection (existing) |
| initialized | bool | Init state flag (existing) |
| **session_backend** | **Arc\<RwLock\<Option\<Arc\<dyn SessionBackend\>\>\>\>** | **Session persistence backend (NEW)** |
| **current_session_key** | **Arc\<Mutex\<Option\<String\>\>\>** | **Active session key tracker for session switching (NEW)** |

**State transitions**: `session_backend` is set during `init()` when `session_persistence` is enabled and a valid `workspace_dir` exists. It may be replaced via `RwLock` write guard during agent rebuild if `workspace_dir` changes via `ConfigPatch`. `current_session_key` tracks which session is currently loaded in memory; updated on each `send_message()` when the session key changes.

### Session (persisted)

A conversation thread stored by the session backend. Not a Rust struct — represented as a sequence of `ChatMessage` records in the backend.

| Attribute | Type | Description |
|-----------|------|-------------|
| session_key | String | Unique identifier, used as filename/DB key |
| messages | Vec\<ChatMessage\> | Ordered sequence of all conversation messages |
| created_at | DateTime | When first message was appended |
| last_activity | DateTime | When most recent message was appended |

**Lifecycle**:
1. **Created** — implicitly when first `send_message()` is called with a session key
2. **Appended** — each turn adds user message + assistant response + tool interactions
3. **Loaded** — on first `send_message()` to an existing session, history is auto-loaded via `seed_history()`
4. **Deleted** — explicitly via `delete_session()`

**Relationships**: Many sessions per workspace. One session backend per `AgentHandle`.

### ChatMessage (existing, unchanged)

| Field | Type | Values |
|-------|------|--------|
| role | String | `"system"`, `"user"`, `"assistant"`, `"tool"` |
| content | String | Message text or JSON-encoded tool data |

### ConversationMessage (existing, unchanged)

| Variant | Fields | Persistence mapping |
|---------|--------|---------------------|
| Chat(ChatMessage) | role, content | Stored as-is |
| AssistantToolCalls | text, tool_calls, reasoning_content | Serialized to ChatMessage(role="assistant") |
| ToolResults | Vec\<ToolResultMessage\> | Serialized to ChatMessage(role="tool") per result |

## Relationships

```
AgentHandle 1──1 Arc<RwLock<Option<Arc<dyn SessionBackend>>>>
    │
    ├── SessionBackend 1──* Session (by session_key)
    │       │
    │       └── Session 1──* ChatMessage (ordered)
    │
    ├── Agent 1──* ConversationMessage (in-memory history)
    │
    ├── current_session_key ──tracks── active Session
    │
    └── RuntimeConfigManager 1──1 Config
            │
            └── Config.workspace_dir → SessionBackend storage root
```

## Conversion Rules

### ConversationMessage → ChatMessage (for persistence)

- `ConversationMessage::Chat(msg)` → `msg` (direct, no conversion)
- `ConversationMessage::AssistantToolCalls { text, tool_calls, reasoning_content }` → `ChatMessage { role: "assistant", content: JSON({ text, tool_calls, reasoning_content }) }`
- `ConversationMessage::ToolResults(results)` → one `ChatMessage { role: "tool", content: JSON(result) }` per result

### ChatMessage → ConversationMessage (for history loading)

> **Note**: This conversion is handled implicitly by `Agent::seed_history(&[ChatMessage])`, which already accepts `ChatMessage` directly. No explicit reverse conversion code is needed in this feature. These rules are documented for reference only.

- `ChatMessage { role: "assistant", content }` where content parses as tool-call JSON → `ConversationMessage::AssistantToolCalls { .. }`
- `ChatMessage { role: "tool", content }` where content parses as tool-result JSON → accumulated into `ConversationMessage::ToolResults(..)`
- All other `ChatMessage` → `ConversationMessage::Chat(msg)`
