# Data Model: Mobile Port with Optional Gateway & Streaming Interface

**Feature**: [spec.md](spec.md)
**Research**: [research.md](research.md)

---

## New Entities

### StreamEvent

Typed events emitted by the streaming conversation interface. Mirrors the existing WebSocket protocol semantics (chunk / tool_call / tool_result / done) and maps 1:1 to Dart via FRB codegen.

```rust
/// A single event emitted by the streaming conversation interface.
///
/// FRB translates this enum into a Dart sealed class hierarchy.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Incremental text from the model response.
    Chunk {
        /// UTF-8 text delta.
        delta: String,
    },

    /// The agent is invoking a tool.
    ToolCall {
        /// Tool name (e.g., "shell", "file_read").
        tool: String,
        /// JSON-encoded arguments (opaque to the host app).
        arguments: String,
    },

    /// A tool invocation has completed.
    ToolResult {
        /// Tool name matching a prior ToolCall.
        tool: String,
        /// Tool output text.
        output: String,
        /// Whether the tool succeeded.
        success: bool,
    },

    /// Agent has finished processing this message.
    Done {
        /// Full aggregated response text.
        full_response: String,
    },

    /// An error occurred during processing.
    Error {
        /// Human-readable error message.
        message: String,
    },
}
```

**Validation rules**: None — this is an output-only type.

**State transitions**:
```
[start] → Chunk* → Done
[start] → Chunk* → ToolCall → ToolResult → Chunk* → … → Done
[start] → Error (terminal)
```

Any number of Chunk events may precede a Done or interleave with ToolCall/ToolResult pairs. Done and Error are terminal — no further events after either. The stream closes after Done or Error.

---

### AgentHandle

Opaque handle to a running ZeroClaw instance. This is the root object the host app creates via FRB. Owns the agent, config manager, and observer registry.

```rust
/// Opaque handle to a running ZeroClaw agent instance.
///
/// Created via `ZeroClaw::init(config)`. The handle is `Send + Sync` and
/// can be shared across threads/isolates.
pub struct AgentHandle {
    /// The underlying agent instance.
    agent: Arc<Mutex<Agent>>,
    /// Runtime config manager with watch-based change notification.
    config_manager: Arc<RuntimeConfigManager>,
    /// Observer callback registry.
    observer_registry: Arc<ObserverCallbackRegistry>,
    /// Cancellation token for aborting in-flight requests.
    cancel_token: CancellationToken,
}
```

**Lifecycle**: Created once at startup. Dropped when the host app releases the handle. Not cloneable by the host — only one handle per instance.

**Concurrency**: Conversations are serial within a single handle — `Arc<Mutex<Agent>>` serializes concurrent `send_message()` calls (R-007). For parallel conversations, create separate `AgentHandle` instances.

---

### RuntimeConfigManager

Manages the live configuration state with validation, persistence, and change notification.

```rust
/// Manages runtime configuration with live reload support.
pub struct RuntimeConfigManager {
    /// Current configuration (protected for concurrent access).
    config: Arc<Mutex<Config>>,
    /// Change notification channel — subsystems subscribe to this.
    tx: tokio::sync::watch::Sender<Config>,
    /// File path for persistence (optional — may be None on Android).
    config_path: Option<PathBuf>,
}
```

**Fields**:

| Field         | Type                    | Purpose                                     |
| ------------- | ----------------------- | ------------------------------------------- |
| `config`      | `Arc<Mutex<Config>>`    | Current validated config                    |
| `tx`          | `watch::Sender<Config>` | Notifies subscribers on change              |
| `config_path` | `Option<PathBuf>`       | TOML file path (None = no file persistence) |

**Validation rules**:
- All updates pass through `Config::validate()` before application
- Invalid updates return `Err(ConfigError)` with field-level error descriptions
- The previous valid config remains active on validation failure

**Key operations**:

| Operation                | Input         | Output                    | Side effects                                                            |
| ------------------------ | ------------- | ------------------------- | ----------------------------------------------------------------------- |
| `get_config()`           | —             | `Config` (clone)          | None                                                                    |
| `update_config(partial)` | `ConfigPatch` | `Result<(), ConfigError>` | Validates → merges → saves to disk (if path set) → notifies subscribers |
| `reload_from_file()`     | —             | `Result<(), ConfigError>` | Re-reads TOML → validates → **merges** (file values overwrite matching fields; absent fields retain in-memory values per R-008) → notifies subscribers |
| `subscribe()`            | —             | `watch::Receiver<Config>` | Returns a new subscriber handle                                         |

---

### ConfigPatch

A partial configuration update. Only specified fields are changed; unspecified fields retain their current values.

```rust
/// Partial configuration update for runtime changes.
///
/// All fields are optional — only `Some` values are applied.
/// FRB translates this to a Dart class with nullable fields.
#[derive(Debug, Clone, Default)]
pub struct ConfigPatch {
    /// Provider name (e.g., "openai", "anthropic").
    pub provider: Option<String>,
    /// Model identifier (e.g., "gpt-4", "claude-3-opus").
    pub model: Option<String>,
    /// API key / token for the provider.
    pub api_key: Option<String>,
    /// API base URL override.
    pub api_base: Option<String>,
    /// Sampling temperature.
    pub temperature: Option<f64>,
    /// System prompt override.
    pub system_prompt: Option<String>,
    /// Maximum tool iterations per turn.
    pub max_tool_iterations: Option<usize>,
    /// Maximum conversation history length.
    pub max_history_messages: Option<usize>,
    // Additional fields mirror Config struct as needed.
    // The full set is generated from the Config schema.
}
```

**Validation rules**:
- `temperature` must be in `[0.0, 2.0]` if set
- `provider` must match a known provider name if set
- `api_key` must be non-empty if set
- `max_tool_iterations` must be `> 0` if set

---

### ObserverCallbackRegistry

Manages host-registered observer callbacks for system-wide event delivery via channels (FRB-compatible — no `dyn Fn` closures in the public API).

```rust
/// Registry of host-provided observer sinks.
///
/// Implements the `Observer` trait so it can be plugged into the
/// existing observability pipeline. Uses mpsc channels for FRB
/// compatibility (StreamSink<ObserverEventDto> on the FRB side).
pub struct ObserverCallbackRegistry {
    /// Registered sinks (id → sender).
    sinks: Arc<Mutex<HashMap<u64, tokio::sync::mpsc::Sender<ObserverEventDto>>>>,
    /// Next sink ID (monotonically increasing).
    next_id: Arc<AtomicU64>,
}
```

**Key operations**:

| Operation             | Input                                   | Output      | Side effects               |
| --------------------- | --------------------------------------- | ----------- | -------------------------- |
| `register(tx)`        | `mpsc::Sender<ObserverEventDto>`        | `u64` (ID)  | Adds sender to registry    |
| `unregister(id)`      | `u64` (ID)                              | —           | Removes sender from registry |

**Note**: FRB translates `StreamSink<ObserverEventDto>` into a Dart `Stream<ObserverEventDto>`. On the Rust side, `register_observer()` in `src/api/observer.rs` creates an `mpsc` channel, stores the sender in the registry, and returns the receiver as a stream to FRB.

---

### ObserverEventDto

FRB-compatible data transfer version of the existing `ObserverEvent` enum.

```rust
/// FRB-compatible observer event for delivery to host app.
#[derive(Debug, Clone)]
pub enum ObserverEventDto {
    AgentStart { provider: String, model: String },
    LlmRequest { provider: String, model: String, messages_count: u32 },
    LlmResponse {
        provider: String,
        model: String,
        duration_ms: u64,
        success: bool,
        error_message: Option<String>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
    },
    AgentEnd {
        provider: String,
        model: String,
        duration_ms: u64,
        tokens_used: Option<u64>,
        cost_usd: Option<f64>,
    },
    ToolCallStart { tool: String, arguments: Option<String> },
    ToolCallEnd { tool: String, duration_ms: u64, success: bool },
    TurnComplete,
    Error { component: String, message: String },
}
```

**Mapping from `ObserverEvent`**: 1:1 except `Duration` → `u64` (milliseconds), `usize` → `u32`. Subset of events — omits internal-only events (HeartbeatTick, CacheHit/Miss, HandStarted/Completed/Failed) that are irrelevant to mobile host apps.

---

## Modified Entities

### Config (existing)

**Changes**: No structural changes. The `Config` struct remains as-is. The `GatewayConfig` section within it continues to exist at the schema level regardless of the `gateway` feature flag (config files should parse without errors even if gateway is compiled out).

### Agent (existing)

**Changes**: `turn_streaming()` **already exists** at `src/agent/agent.rs` L808-1045. It takes `tokio::sync::mpsc::Sender<StreamEvent>`, `CancellationToken`, and an `observer_registry` parameter. No new method needed — `src/api/conversation.rs` wraps this existing method.

```rust
impl Agent {
    /// Existing: process a message, return full response.
    pub async fn turn(&mut self, message: &str) -> Result<String> { /* unchanged */ }

    /// Existing: process a message, stream events to the provided channel.
    /// Already implemented at L808-1045.
    pub async fn turn_streaming(
        &mut self,
        message: &str,
        event_tx: tokio::sync::mpsc::Sender<StreamEvent>,
        cancel_token: CancellationToken,
        observer_registry: &ObserverCallbackRegistry,
    ) -> Result<()> { /* existing */ }
}
```

The `turn()` method remains unchanged for backward compatibility. `turn_streaming()` uses the same internal tool loop but emits `StreamEvent`s at each step. Currently calls `provider.chat()` (non-streaming); true provider-level streaming is a future enhancement.

---

## Entity Relationship Diagram

```
┌─────────────┐
│ AgentHandle  │──────────────────────────────────────────────┐
│              │                                              │
│  .send_msg() │→ Agent::turn_streaming() → mpsc::Sender ─→ StreamEvent*
│              │                                              │
│  .update()   │→ RuntimeConfigManager                        │
│  .reload()   │    ├── Config (validated)                    │
│  .get()      │    ├── watch::Sender<Config>                 │
│              │    └── Option<PathBuf>                        │
│              │                                              │
│  .register() │→ ObserverCallbackRegistry ──→ ObserverEventDto*
└─────────────┘
```

- **AgentHandle** owns Agent, RuntimeConfigManager, ObserverCallbackRegistry
- **Agent::turn_streaming()** produces `StreamEvent` items via mpsc channel
- **RuntimeConfigManager** notifies subsystems of config changes via `watch` channel
- **ObserverCallbackRegistry** delivers `ObserverEventDto` to registered host callbacks
- **ConfigPatch** is the input to `RuntimeConfigManager::update_config()`
