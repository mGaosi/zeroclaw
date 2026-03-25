# Data Model: Host-Side Tool Registration

**Feature**: 002-host-tool-registration
**Date**: 2026-03-24

## Entities

### HostToolSpec

The definition of a host-side tool as provided by the host app during registration. This is the **public API type** — all fields are plain strings for FRB compatibility.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | `String` | Yes | Unique tool name (used in LLM function calling). Must not collide with built-in tool names. |
| `description` | `String` | Yes | Human-readable description shown to the LLM. Must be non-empty. |
| `parameters_schema` | `String` | Yes | JSON-encoded parameter schema (JSON Schema subset). Must parse as a valid JSON object. |
| `timeout_seconds` | `Option<u32>` | No | Per-tool timeout override. Defaults to 30 seconds if not specified. |

**Validation rules**:
- `name`: Non-empty, no whitespace-only, must not match any built-in tool name or existing host tool name (FR-009)
- `description`: Non-empty
- `parameters_schema`: Must parse as valid JSON; top-level value must be a JSON object (FR-010)
- `timeout_seconds`: If provided, must be > 0

**Serialization**: `#[derive(Debug, Clone, Serialize, Deserialize)]`

---

### ToolRequest

A structured message sent from ZeroClaw to the host app when the LLM decides to invoke a host tool. Delivered via the shared `mpsc` channel.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `request_id` | `String` | Yes | UUID v4 identifying this specific invocation. Used to correlate with `ToolResponse`. |
| `tool_name` | `String` | Yes | The name of the host tool to execute. |
| `arguments` | `String` | Yes | JSON-encoded arguments from the LLM. |

**Serialization**: `#[derive(Debug, Clone, Serialize, Deserialize)]`

**Lifecycle**:
1. Created in `HostToolProxy::execute()` when the LLM invokes the tool
2. Sent to the host app via `mpsc::UnboundedSender<ToolRequest>`
3. Consumed by the host app, which processes it and returns a `ToolResponse`

---

### ToolResponse

A structured message sent from the host app back to ZeroClaw with the result of a tool execution.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `request_id` | `String` | Yes | Must match the `request_id` from the corresponding `ToolRequest`. |
| `output` | `String` | Yes | Tool output text (may be empty on failure). |
| `success` | `bool` | Yes | Whether the tool execution succeeded. |

**Serialization**: `#[derive(Debug, Clone, Serialize, Deserialize)]`

**Validation rules**:
- `request_id`: Must match a pending request (if not, the response is silently discarded — the request may have timed out or been cancelled)

---

### HostToolRegistry

The runtime registry that tracks all currently registered host tools and manages the execution channel. Lives in `AgentHandle` alongside `ObserverCallbackRegistry`.

| Field | Type | Description |
|-------|------|-------------|
| `request_tx` | `Arc<Mutex<mpsc::UnboundedSender<ToolRequest>>>` | Outbound channel sender for all tool requests. Wrapped in `Arc<Mutex<>>` so `reset_channel()` can swap to a fresh sender and existing proxies automatically pick up the change (FR-014). |
| `request_rx` | `Mutex<Option<mpsc::UnboundedReceiver<ToolRequest>>>` | Receiver half, taken when `setup_tool_handler` is called. Repopulated by `reset_channel()` for re-establishment (FR-014). |
| `pending` | `Arc<Mutex<HashMap<String, oneshot::Sender<ToolResponse>>>>` | Pending request map keyed by `request_id`. |
| `tools` | `Arc<Mutex<HashMap<u64, HostToolMeta>>>` | Registered tool definitions keyed by registration ID. |
| `next_id` | `Mutex<u64>` | Monotonically increasing registration ID counter. |

**Internal type** (not in public API): `HostToolMeta`

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Tool name |
| `description` | `String` | Tool description |
| `parameters_schema` | `serde_json::Value` | Parsed JSON schema (parsed once at registration, stored as `Value` internally) |
| `timeout` | `Duration` | Effective timeout for this tool |

---

### HostToolProxy

The Rust-side proxy that implements the `Tool` trait for a single host-registered tool. Created by `HostToolRegistry::create_proxies()`.

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Tool name (immutable after construction) |
| `description` | `String` | Tool description (immutable after construction) |
| `parameters_schema` | `serde_json::Value` | JSON schema (immutable after construction) |
| `request_tx` | `Arc<Mutex<mpsc::UnboundedSender<ToolRequest>>>` | Shared reference to registry's outbound sender. Uses `Arc<Mutex<>>` so channel re-establishment (FR-014) is transparent to existing proxies. |
| `pending` | `Arc<Mutex<HashMap<String, oneshot::Sender<ToolResponse>>>>` | Shared reference to registry's pending map |
| `timeout` | `Duration` | Timeout for this tool's execution |
| `cancel_token` | `Option<tokio_util::sync::CancellationToken>` | Agent-level cancellation token. When fired, the `biased` `select!` in `execute()` returns a cancellation failure immediately (FR-012). |

**Note**: `HostToolProxy` holds an optional `CancellationToken` from `AgentHandle`, passed at construction time via `create_proxies()`. When the token fires (shutdown or turn cancellation), the `biased` `select!` in `execute()` picks up the cancellation immediately, returning a failure `ToolResult` and cleaning up the pending map entry.

---

## Relationships

```
AgentHandle
├── agent: Arc<Mutex<Agent>>
│   └── tools: Vec<Box<dyn Tool>>
│       └── [HostToolProxy, HostToolProxy, ...] ← injected from registry
├── observer_registry: Arc<ObserverCallbackRegistry>    (existing)
└── host_tool_registry: Arc<HostToolRegistry>            (NEW)
    ├── tools: HashMap<u64, HostToolMeta>
    ├── request_tx → mpsc → host app (Dart)
    └── pending: HashMap<String, oneshot::Sender<ToolResponse>>
```

## State Transitions

### Tool Lifecycle

```
                  register_tool()
    [Not Registered] ──────────────► [Registered]
                                        │
                                        │ LLM invokes
                                        ▼
                                   [Executing]
                                    ╱    │    ╲
                          timeout  ╱     │     ╲  cancelled
                                 ╱      │      ╲
                               ▼       ▼       ▼
                         [Failed]  [Completed]  [Failed]
                                        │
                                        │ (remains registered)
                                        ▼
                                   [Registered]
                                        │
                                        │ unregister_tool()
                                        ▼
                                 [Not Registered]
```

### Agent Rebuild Flow

```
Config change detected
        │
        ▼
Agent::from_config()  ← creates new Agent (no host tools)
        │
        ▼
agent.replace_host_tools(registry.create_proxies())  ← re-injects host tools
        │
        ▼
Agent ready with host tools intact
```

### Channel Lifecycle

```
init()
  │
  ▼
HostToolRegistry::new()  ← creates mpsc channel pair (sender in Arc<Mutex<>>)
  │
  ▼
setup_tool_handler(handle, sink)  ← takes rx, bridges to FRB StreamSink
  │
  ▼
[Channel active]  ← ToolRequests flow to host, ToolResponses flow back
  │
  ├── (channel closes, e.g. app backgrounded)
  │         │
  │         ▼
  │   setup_tool_handler(handle, new_sink)  ← calls reset_channel(),
  │         │                                   creates fresh mpsc pair,
  │         │                                   swaps sender under lock,
  │         │                                   takes new rx (FR-014)
  │         │
  │         ▼
  │   [Channel active]  ← new requests use new channel;
  │                        in-flight on old channel get "channel closed"
  │
  ▼
shutdown()  ← drops registry, closes channels, pending requests fail
```
