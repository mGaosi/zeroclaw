# Research: Host-Side Tool Registration

**Feature**: 002-host-tool-registration
**Date**: 2026-03-24

## R-001: HostToolProxy — Delegating Tool Trait to Host Callback Channel

**Decision**: Each registered host tool is represented by a `HostToolProxy` struct that implements the `Tool` trait. All proxies share a single `mpsc::UnboundedSender<ToolRequest>` for outbound requests and a shared `Arc<Mutex<HashMap<String, oneshot::Sender<ToolResponse>>>>` for correlating responses by request ID.

**Rationale**: The FRB layer cannot translate `dyn Tool` across FFI. The proxy pattern keeps all trait-object complexity on the Rust side while exposing only concrete, serializable types (`ToolRequest`, `ToolResponse`) to the host. The `mpsc::UnboundedSender` is cheap to clone (internally an `Arc`) and the `parking_lot::Mutex` on the pending map is held only for microsecond-level `HashMap::insert`/`remove` operations — never across `.await` points.

**Alternatives considered**:
- **Per-tool channels**: Each registered tool gets its own `mpsc` channel. Rejected — more channels means more FRB `StreamSink` bindings, more Dart `Stream` listeners, and more complexity on both sides.
- **Shared channel with enum dispatch**: Single channel carrying tagged unions. Rejected — adds discriminant overhead and requires the host to demux; the request-ID approach is simpler.
- **Ring buffer / lock-free queue**: Rejected — overkill for <20 tools with sub-1ms dispatch; standard library channels are sufficient.

## R-002: Request/Response Correlation with Timeout and Cancellation

**Decision**: Use `tokio::sync::oneshot` channels for per-request correlation. On `execute()`:
1. Generate a UUID v4 request ID (using existing `uuid` crate)
2. Create a `oneshot::channel()` pair
3. Insert the sender into the pending map **before** sending the request (prevents race)
4. Send `ToolRequest` via shared `mpsc`
5. `tokio::select!` with `biased` on: cancellation token, `tokio::time::timeout(duration, rx)`

**Rationale**: `oneshot` is zero-cost after creation — the sender is consumed on send, no cleanup needed. The `biased` select ensures cancellation is always checked first. Timeout defaults to 30 seconds (configurable per-tool at registration time). All three exit paths (success, timeout, cancellation) clean up the pending map entry.

**Alternatives considered**:
- **Callback-based**: Host registers a callback function per tool. Rejected — callbacks can't cross FRB boundary as closures.
- **Polling**: Proxy polls a shared response queue. Rejected — wastes CPU and adds latency.
- **Atomic request ID counter (u64)**: Instead of UUID. Viable but UUID is already a dependency and avoids any rollover concerns over long sessions.

## R-003: Host Tool Persistence Across Agent Rebuilds

**Decision**: `HostToolRegistry` lives in `AgentHandle` (alongside `ObserverCallbackRegistry`), not inside `Agent`. After `apply_config_changes_if_needed` creates a new `Agent` via `from_config()`, it calls `agent.replace_host_tools(registry.create_proxies(...))` to re-inject host tool proxies.

**Rationale**: `Agent::from_config()` builds a fresh tool list from config. Host tools are an API-layer concept, not a config concept — they shouldn't pollute `from_config`'s signature. The `replace_host_tools(&mut self, host_tools: Vec<Box<dyn Tool>>)` method follows the same mutation pattern as the existing `set_observer(&mut self, observer)` method on Agent. Unlike a simple `add_tools`, `replace_host_tools` stores host tools separately from built-in tools and rebuilds the combined list, so unregistrations are properly reflected.

**Alternatives considered**:
- **`from_config_with_extra_tools(config, extra_tools)`**: Bloats `from_config` signature for a single caller's concern. Rejected — violates Minimal Patch principle.
- **Store host tools inside `Agent`**: Would be lost on every rebuild. Rejected — contradicts FR-008.
- **Modify config to carry host tool definitions**: Config should not carry runtime state. Rejected.

## R-004: FRB Compatibility Constraints

**Decision**: Public API exposes only concrete, serializable structs:
- `HostToolSpec { name: String, description: String, parameters_schema: String }` — schema as JSON string (not `serde_json::Value`, which FRB can't translate)
- `ToolRequest { request_id: String, tool_name: String, arguments: String }` — arguments as JSON string
- `ToolResponse { request_id: String, output: String, success: bool }` — simple concrete types

FRB wrappers (behind `#[cfg(feature = "frb")]`):
- `register_tool_stream(handle, sink: StreamSink<ToolRequest>) -> Result<(), ApiError>` — sets up the tool request stream
- `submit_tool_response(handle, response: ToolResponse) -> Result<(), ApiError>` — host returns results
- `register_tool(handle, spec: HostToolSpec) -> Result<u64, ApiError>` — register a tool
- `unregister_tool(handle, tool_id: u64) -> Result<(), ApiError>` — unregister

**Rationale**: Follows the exact same pattern as `register_observer_stream` in `src/api/observer.rs`. FRB translates `StreamSink<ToolRequest>` into a Dart `Stream<ToolRequest>`. The host app listens on one stream for all tool requests, processes them, and calls `submit_tool_response` for each.

**Alternatives considered**:
- **Expose `serde_json::Value` in public API**: FRB can't translate this. Rejected.
- **Separate stream per tool**: More streams = more FRB bindings = more complexity. Rejected per clarification decision.
- **Bidirectional stream**: Single stream carrying both requests and responses. Rejected — responses need to be submitted synchronously via function call, not streamed.

## R-005: Name Collision Detection with Built-in Tools

**Decision**: At registration time, `HostToolRegistry::register()` receives the current list of built-in tool names and checks for duplicates. The built-in tool names are obtained from `Agent.tools` (via a new `tool_names()` accessor or by querying at registration time from the handle).

**Rationale**: FR-009 requires rejecting duplicate names. Checking at registration time (not at turn time) prevents confusing behavior where a tool appears registered but silently fails. The check covers both built-in tool names and already-registered host tool names.

**Alternatives considered**:
- **Shadow with priority**: Host tools override built-in tools. Rejected — spec explicitly forbids shadowing.
- **Namespace prefix**: All host tools automatically prefixed with `host_`. Rejected — adds complexity to LLM prompts and tool identification.
- **Turn-time check**: Check when building tool catalog for LLM. Rejected — late failure is worse than early rejection.

## R-006: Schema Validation

**Decision**: Basic validation at registration time: the provided schema must parse as valid JSON and must be a JSON object at the top level (matching the `parameters_schema()` return type in the `Tool` trait). Deep JSON Schema validation (e.g., validating `$ref`, `allOf`) is NOT performed — the LLM function-calling format only requires a shallow object with property definitions.

**Rationale**: Over-validating schemas would require pulling in a JSON Schema validator crate (heavy dependency, violates Constitution III). The existing built-in tools also don't deeply validate their own schemas — they just return `serde_json::json!({...})` objects.

**Alternatives considered**:
- **Full JSON Schema validation (jsonschema crate)**: Heavy dependency for marginal benefit. Rejected.
- **No validation**: Would allow malformed schemas that confuse the LLM. Rejected — FR-010 requires validation.

## R-007: Cancellation on Shutdown

**Decision**: The `CancellationToken` from `AgentHandle` is passed to each `HostToolProxy` at construction time. When shutdown is triggered, the token fires, and the `biased` `select!` in `execute()` picks up the cancellation immediately, returning a failure `ToolResult`.

**Rationale**: Follows the existing cancellation pattern used throughout the agent — `turn_streaming` already accepts and respects a `CancellationToken`. No new cancellation mechanism needed.

**Alternatives considered**:
- **Separate cancellation per host tool**: Over-engineered — the agent-level token is sufficient since all host tools should stop when the agent stops.
- **Drop-based cleanup**: Drop the `HostToolRegistry` to cancel all pending. Works but less controlled — `CancellationToken` gives explicit, graceful shutdown.

## R-008: Channel Re-establishment (FR-014)

**Decision**: `setup_tool_handler` is callable more than once. On each call after the first, the registry creates a fresh unbounded `mpsc` channel pair, swaps the sender stored in `Arc<Mutex<mpsc::UnboundedSender>>`, and returns the new receiver to the host. Existing `HostToolProxy` instances transparently pick up the new sender through the shared `Arc<Mutex<>>` — no re-registration of tools is needed.

**Rationale**: Mobile platforms (iOS/Android) may suspend or terminate the host process's event loop when backgrounded. The Dart Stream backed by the original receiver becomes invalid after the app resumes. Allowing `setup_tool_handler` to be called again lets the host re-attach a fresh listener without losing registry state. In-flight invocations on the old channel receive a "channel closed" error, which is the correct semantic since the old listener is gone.

**Alternatives considered**:
- **Reconnectable channel abstraction**: Wrap the channel in a reconnectable layer that buffers messages during disconnection. Rejected — adds complexity and violates Constitution III (minimal dependencies). Buffering stale tool requests from before suspend is not useful.
- **Full teardown and re-setup**: Require `shutdown()` + `init()` + re-register all tools. Rejected — destroys registry state unnecessarily and forces the host to track and replay registrations.
- **Single-use handler (status quo before FR-014)**: Keep the once-only constraint. Rejected — breaks real-world mobile app lifecycle where backgrounding is expected.
