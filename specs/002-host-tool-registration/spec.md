# Feature Specification: Host-Side Tool Registration for Flutter

**Feature Branch**: `002-host-tool-registration`
**Created**: 2026-03-24
**Status**: Draft
**Input**: User description: "api 支持在flutter 层面实现一些tool 让zeroclaw调用"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Flutter App Registers a Custom Tool (Priority: P1)

A Flutter developer building a mobile app with ZeroClaw wants the agent to interact with device-specific capabilities — for example, reading the phone's GPS location, accessing the camera, or querying a local database. The developer defines a tool with a name, description, and parameter schema on the Flutter/Dart side, then registers it with ZeroClaw via the library API. When the agent decides to invoke that tool during a conversation, ZeroClaw sends a tool execution request to the host app, the host app performs the action and returns the result, and ZeroClaw incorporates the result into its response.

**Why this priority**: This is the foundational capability. Without the ability to register a host-side tool and have ZeroClaw invoke it, none of the other stories (dynamic registration, lifecycle management) are possible. A single registered tool completing a round-trip proves the entire mechanism works.

**Independent Test**: Initialize ZeroClaw. Register a simple tool (e.g., "get_device_battery") with a known schema. Send a message that triggers the agent to invoke the tool. Verify the host app receives the tool execution request with correct arguments, returns a result, and the agent's final response incorporates the tool output.

**Acceptance Scenarios**:

1. **Given** ZeroClaw is initialized, **When** the host app registers a tool with a name, description, and parameter schema, **Then** the agent's tool catalog includes the new tool and the LLM can discover it.
2. **Given** a host tool is registered, **When** the agent decides to call it during a conversation, **Then** the host app receives the tool name and arguments as a structured request and can return a structured result.
3. **Given** the host app returns a tool result, **When** ZeroClaw receives it, **Then** the result is fed back into the agent's conversation turn and the agent continues generating its response incorporating the tool output.
4. **Given** a host tool is registered, **When** the conversation stream is active, **Then** `ToolCall` and `ToolResult` events for host tools are emitted to the stream in the same format as built-in tools.

---

### User Story 2 - Dynamic Tool Registration and Unregistration (Priority: P2)

A Flutter developer needs to add and remove tools during the app's lifecycle — for example, registering a "scan_barcode" tool only when the camera screen is active, or removing a "bluetooth_discover" tool when Bluetooth is turned off. The developer can register new tools and unregister existing ones at any time without restarting the agent.

**Why this priority**: Dynamic lifecycle management is essential for real-world mobile apps where capabilities come and go based on context (permissions granted, screens navigated, hardware states). Without this, tools must all be registered at startup, limiting flexibility.

**Independent Test**: Initialize ZeroClaw. Register tool A. Verify it's available. Unregister tool A. Verify it's no longer in the agent's tool catalog. Register tool B while a conversation is idle. Send a message and verify tool B is available.

**Acceptance Scenarios**:

1. **Given** a tool is registered, **When** the host app calls unregister with the tool's identifier, **Then** the tool is removed from the agent's catalog and the LLM can no longer invoke it.
2. **Given** ZeroClaw is running with no host tools, **When** the host app registers a new tool, **Then** the tool becomes available for the next conversation turn without restarting the agent.
3. **Given** a tool is unregistered while no conversation is in flight, **When** the next conversation turn starts, **Then** the agent's tool catalog reflects the removal.

---

### User Story 3 - Host Tool Participates in Streaming Conversation (Priority: P2)

A Flutter developer wants the chat UI to show real-time feedback when a host tool is being invoked — including showing the tool name and arguments before execution, and the result after. The streaming event protocol should treat host tools identically to built-in tools, so the UI rendering logic does not need special cases.

**Why this priority**: Consistent streaming behavior is critical for a polished mobile UX. If host tools behave differently from built-in tools in the event stream, the Flutter UI would need separate rendering paths, increasing complexity and bugs.

**Independent Test**: Register a host tool. Send a message that triggers it. Observe the streaming events and verify `ToolCall` (with tool name and arguments) arrives before execution, `ToolResult` (with output and success status) arrives after, and no new event types are introduced.

**Acceptance Scenarios**:

1. **Given** a host tool is invoked during a conversation, **When** the stream emits events, **Then** a `ToolCall` event with the host tool's name and arguments is emitted before execution begins.
2. **Given** a host tool execution completes, **When** the result is returned to ZeroClaw, **Then** a `ToolResult` event with the output and success status is emitted in the stream.
3. **Given** multiple tools (both built-in and host-registered) are invoked in a single turn, **When** the stream emits events, **Then** all tool events follow the same `ToolCall → ToolResult` ordering regardless of origin.

---

### Edge Cases

- What happens when a host tool execution times out (the host app never returns a result)? The system MUST enforce a configurable timeout and treat expiry as a tool failure with an error message, allowing the agent to continue.
- What happens when a host tool is unregistered while it is being invoked in an in-flight conversation turn? The in-flight invocation MUST complete (or timeout) normally; the removal takes effect starting from the next turn.
- What happens when the host app registers a tool with the same name as a built-in tool? Registration MUST be rejected with a clear error — host tools MUST NOT shadow built-in tools.
- What happens when the host app registers a tool with an invalid parameter schema? Registration MUST be rejected with a validation error describing the schema issue.
- What happens when the agent rebuilds due to a config change (e.g., provider switch)? Host-registered tools MUST survive agent rebuilds — they are not lost when the configuration changes.
- What happens when `shutdown()` is called while a host tool execution is pending? The pending execution receives a cancellation signal and the shutdown proceeds.
- What happens when the host app returns a malformed or empty result from a tool execution? The system treats it as a tool failure and reports the error to the agent, which decides how to proceed.
- What happens when the host app's tool handler channel closes (e.g., app backgrounded on mobile)? `setup_tool_handler` MUST be callable again to re-establish the channel. The registry creates a fresh internal channel pair and updates the request sender used by existing proxies. Any in-flight invocations on the old channel receive a "channel closed" error.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST expose a tool registration interface that allows the host app to register custom tools by providing a tool name, description, and parameter schema.
- **FR-002**: The system MUST expose a tool unregistration interface that removes a previously registered host tool by its identifier.
- **FR-003**: Host-registered tools MUST be discoverable by the LLM — their specifications (name, description, parameter schema) MUST be included in the tool catalog sent to the model alongside built-in tools.
- **FR-004**: When the LLM decides to invoke a host-registered tool, the system MUST send a structured execution request (tool name, arguments) to the host app and wait for a structured result (output text, success/failure status).
- **FR-005**: Host tool execution requests MUST be delivered to the host app via a single shared asynchronous unbounded channel, with individual requests distinguished by a unique request identifier. The channel MUST be compatible with the existing FRB bridging pattern (allowing translation to a single Dart Stream that carries all tool requests). The unbounded channel is intentional given the expected scale (1–20 tools, single concurrent invocation per turn).
- **FR-006**: Host tool execution MUST have a configurable timeout. If the host app does not return a result within the timeout, the system MUST treat the invocation as a failure and report an error to the agent.
- **FR-007**: Host-registered tools MUST participate in the same streaming event pipeline as built-in tools — `ToolCall` and `ToolResult` events for host tools MUST be emitted to the conversation stream.
- **FR-008**: Host-registered tools MUST persist across agent rebuilds triggered by configuration changes. When the agent is reconstructed (e.g., after a provider switch), all currently registered host tools MUST be re-injected into the new agent instance. Re-injected proxy instances share the request sender via `Arc<Mutex<>>`, so they always use the current channel — whether it is the original or one re-established via FR-014. The host app does NOT need to call `setup_tool_handler` again after a rebuild; only after the handler channel itself is lost (e.g., app backgrounded).
- **FR-009**: Registration of a host tool with the same name as an existing built-in or previously registered host tool MUST be rejected with a descriptive error.
- **FR-010**: The tool parameter schema provided during registration MUST be validated. Invalid schemas MUST be rejected with a clear error.
- **FR-011**: Host tool registration and unregistration MUST be safe to call from any thread and MUST NOT block the agent's event loop.
- **FR-012**: In-flight host tool invocations MUST be cancellable — if the conversation is cancelled or the agent shuts down, pending host tool requests MUST receive a cancellation signal.
- **FR-013**: Host tool registration MUST require a valid, initialized agent instance. Registration attempts before initialization completes MUST be rejected with a clear error.
- **FR-014**: `setup_tool_handler` MUST be callable more than once to support channel re-establishment after the previous handler channel closes (e.g., app backgrounded on mobile). On the first call, the registry creates the initial channel pair and returns the receiver. On each subsequent call, the registry creates a fresh channel pair, swaps the sender in the shared `Arc<Mutex<>>`, and returns the new receiver. Existing `HostToolProxy` instances transparently use the new sender — no re-registration is needed. In-flight invocations on the old channel receive a "channel closed" error.

### Key Entities

- **HostToolSpec**: The definition of a host-side tool — includes a unique name, human-readable description, and a JSON parameter schema. Provided by the host app during registration. Used to generate the tool specification sent to the LLM.
- **ToolRequest**: A structured message sent from ZeroClaw to the host app when the LLM decides to invoke a host tool. Contains the tool name, a request identifier, and the JSON-encoded arguments.
- **ToolResponse**: A structured message sent from the host app back to ZeroClaw with the result of a tool execution. Contains the request identifier, output text, and a success/failure indicator.
- **HostToolRegistry**: The runtime registry that tracks all currently registered host tools, their specifications, and the shared execution channel. Uses a single request/response channel multiplexed by request ID. Analogous to the existing ObserverCallbackRegistry.

## Assumptions

- This feature depends on the `001-android-port-streaming` feature (library API, AgentHandle, streaming interface) being implemented and available. The host tool registration API extends the existing `src/api/` module.
- The host app (Flutter/Dart) is responsible for implementing the actual tool logic. ZeroClaw only sends structured requests and receives structured results — it does not execute host tool code.
- The parameter schema format follows the JSON Schema subset used by LLM function calling (matching the existing `ToolSpec.parameters` format used by built-in tools).
- Host tools are implicitly trusted and bypass autonomy-level checks (ReadOnly, Supervised, Full). The host app registers and executes the tools itself, so Supervised-mode approval is unnecessary — it would create a confusing loop where the host approves its own tools. Built-in tool security policy (workspace isolation, command allowlisting) does not apply to host tools since execution happens on the Dart/Flutter side, outside the Rust sandbox.
- The host app is responsible for ensuring tool execution does not block indefinitely. The system-side timeout is a safety net, not a substitute for responsible host-side implementation.
- A reasonable default timeout of 30 seconds is assumed for host tool execution. This is configurable per-tool or globally.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A host-registered tool is invoked by the agent and returns a correct result within a single conversation turn, verified end-to-end without any built-in tool involvement.
- **SC-002**: Tools can be registered and unregistered dynamically — the agent's tool catalog updates within the same session without restart.
- **SC-003**: Host tool events (`ToolCall`, `ToolResult`) appear in the conversation stream indistinguishably from built-in tool events.
- **SC-004**: Host-registered tools survive an agent rebuild triggered by `update_config()` — a tool registered before a config change remains available after the change takes effect.
- **SC-005**: A host tool that exceeds the configured timeout is treated as a failure, and the agent continues the conversation without hanging.
- **SC-006**: Attempting to register a tool with a duplicate name returns a clear error and does not corrupt the existing tool catalog.
- **SC-007**: All host tool registration and invocation APIs pass the existing FRB compatibility criteria (concrete types, no trait objects in public API, no raw pointers).

## Clarifications

### Session 2026-03-24

- Q: Should host tools go through Supervised-mode approval like built-in tools, or be implicitly trusted? → A: Host tools are implicitly trusted — no autonomy-level check on dispatch. The host app controls both registration and execution, so approval would be redundant.
- Q: Should each registered tool have its own execution channel, or a single shared channel multiplexed by request ID? → A: Single shared channel with request IDs. Simpler for the host app (one listener), lower overhead, mirrors the ObserverCallbackRegistry pattern.
- Q: Can host tools be registered before init() completes, or only after? → A: Registration only after init() — the host app must wait for a valid AgentHandle. Matches the existing API pattern where all operations require an initialized handle.

### Session 2026-03-25

- Q: When the host app's tool handler channel closes (e.g., app backgrounded on mobile), should `setup_tool_handler` be callable again to re-establish the channel? → A: Yes — the handler channel should be re-connectable. On mobile, app lifecycle events make once-only channel setup fragile. `setup_tool_handler` must be callable again after the previous channel closes. The registry should create a fresh internal channel pair and update the request sender used by existing proxies.
