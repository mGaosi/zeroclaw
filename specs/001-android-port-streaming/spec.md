# Feature Specification: Mobile Port (Android + iOS) with Optional Gateway & Streaming Interface

**Feature Branch**: `001-android-port-streaming`
**Created**: 2026-03-19
**Status**: Draft
**Input**: User description: "gateway 模块改为 feature 可选，将 ZeroClaw 移植到 Android 系统，支持配置文件与直接接口调整配置并实时生效。对外暴露一些对话功能接口，接口层面能直接流式输入输出。"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Android App Embeds ZeroClaw as Library (Priority: P1)

An Android application developer integrates ZeroClaw as a native library into their app. They initialize the agent runtime without the gateway (HTTP server) component, keeping the binary size small and avoiding unnecessary network surface. The app calls the conversation interface directly from its UI layer, sending user messages and receiving streamed response chunks in real time.

**Why this priority**: This is the foundational capability — without the ability to run ZeroClaw on Android as a library (without gateway), none of the other stories are possible. It unblocks the entire Android ecosystem.

**Independent Test**: Build ZeroClaw for an Android target with the gateway feature disabled. Initialize the runtime from a test harness. Send a message through the conversation interface and receive a complete streamed response — confirms core functionality on Android without HTTP.

**Acceptance Scenarios**:

1. **Given** a ZeroClaw build compiled for Android (aarch64-linux-android) with `default-features = false` (gateway excluded), **When** the host app initializes the agent runtime with a valid config, **Then** the runtime starts successfully without binding any network ports.
2. **Given** a running ZeroClaw instance on Android, **When** the host app sends a conversation message via the library interface, **Then** the agent processes the message and returns response chunks incrementally (streamed) rather than waiting for the full response.
3. **Given** ZeroClaw is running on Android, **When** the host app sends a message, **Then** tool calls and their results are also streamed as discrete events alongside text chunks.

---

### User Story 2 - Runtime Configuration via Interface (Priority: P1)

A developer using ZeroClaw on Android needs to change configuration at runtime — for example, switching the model provider, adjusting temperature, or toggling a channel — without restarting the agent. They call a configuration update interface with the new values, and the changes take effect immediately for subsequent interactions.

**Why this priority**: Equal priority with Story 1 because configuration management is essential for a usable embedded agent. Without runtime config changes, every adjustment requires a full restart — unacceptable for a mobile app where the agent should feel responsive.

**Independent Test**: Start ZeroClaw with provider A configured. Call the config update interface to switch to provider B. Send a message and verify the response comes from provider B — no restart involved.

**Acceptance Scenarios**:

1. **Given** ZeroClaw is running with a loaded config, **When** the host app updates a configuration value via the programmatic interface, **Then** the change takes effect for the next interaction without requiring a restart.
2. **Given** a configuration change is applied at runtime, **When** the change affects an active subsystem (e.g., provider, channel), **Then** the subsystem reinitializes transparently with the new settings.
3. **Given** an invalid configuration value is submitted, **When** the validation check runs, **Then** the change is rejected with a clear error, and the previous valid configuration remains active.

---

### User Story 3 - Configuration File Loading on Android (Priority: P2)

A developer deploys ZeroClaw on Android and provides a configuration file (TOML) bundled with the app or stored in the app's private storage. On startup, ZeroClaw loads this file. The developer can also update the file at runtime and trigger a reload, which merges the file changes with the current running configuration.

**Why this priority**: Config file support is important for deployment flexibility (pre-baked configs, MDM-managed configs), but the programmatic interface (Story 2) covers the most urgent runtime use case. File-based config is complementary.

**Independent Test**: Place a config TOML in the app's data directory. Start ZeroClaw — it loads from the file. Modify the file and trigger a reload — verify the new values are active.

**Acceptance Scenarios**:

1. **Given** a valid config file exists at the configured path, **When** ZeroClaw starts, **Then** it loads and applies the configuration from that file.
2. **Given** ZeroClaw is running, **When** the host app triggers a config file reload, **Then** the file is re-read, validated, merged with current in-memory config, and applied live.
3. **Given** the config file contains invalid values, **When** a reload is triggered, **Then** the reload fails gracefully with a descriptive error and the running config remains unchanged.

---

### User Story 4 - Gateway as Optional Feature (Priority: P2)

A desktop/server user wants the full ZeroClaw experience including the web dashboard, webhook integrations, and WebSocket-based chat. They compile with the gateway feature enabled and get all existing gateway functionality unchanged. Meanwhile, Android or embedded users exclude the gateway to reduce binary size and attack surface.

**Why this priority**: Making gateway optional is an architectural prerequisite for a lean Android build. However, it's P2 because the gateway already works — the work here is refactoring it behind a feature flag without breaking existing users.

**Independent Test**: Build ZeroClaw twice: once with `--features gateway` and once without. Verify the gateway build has all HTTP endpoints working. Verify the non-gateway build compiles, runs, and has no HTTP-related dependencies linked.

**Acceptance Scenarios**:

1. **Given** ZeroClaw is compiled with the `gateway` feature enabled, **When** the user starts the gateway command, **Then** all existing HTTP/WebSocket endpoints work exactly as before (no regressions).
2. **Given** ZeroClaw is compiled without the `gateway` feature, **When** the binary is inspected, **Then** no HTTP server dependencies (Axum, Tower-HTTP, rust-embed) are linked, and binary size is measurably smaller.
3. **Given** the `gateway` feature is disabled, **When** the user attempts to run the `gateway` CLI command, **Then** a clear error message indicates the feature is not available in this build.

---

### User Story 5 - Streaming Conversation Events from Library Interface (Priority: P2)

A mobile app developer building a chat UI needs fine-grained streaming events from ZeroClaw — not just text chunks, but also tool invocations, tool results, and completion signals. The interface delivers these as a typed event stream so the app can render tool activity, progress indicators, and final responses distinctly.

**Why this priority**: Streaming text alone is functional but basic. Rich event streaming (tool calls, results, done) elevates the UX and matches the existing WebSocket protocol quality — critical for professional-grade Android integrations.

**Independent Test**: Send a message that triggers a tool call. Verify the stream delivers: text chunks, a tool_call event, a tool_result event, and a done event — all in order and with correct data.

**Acceptance Scenarios**:

1. **Given** a conversation is active, **When** the agent generates a response, **Then** the stream emits `chunk` events containing incremental text.
2. **Given** the agent invokes a tool during response generation, **When** the tool call begins, **Then** the stream emits a `tool_call` event with the tool name and arguments, followed by a `tool_result` event with the outcome.
3. **Given** the agent has completed its response, **When** all chunks and tool results are sent, **Then** the stream emits a `done` event containing the full aggregated response.

---

### Edge Cases

- What happens when the Android app is backgrounded while ZeroClaw is processing a request? The runtime must handle the host process being suspended — in-flight requests should complete or be safely cancellable.
- What happens when the config file path does not exist on Android? Startup should succeed with built-in defaults and log a warning, not crash.
- What happens when a runtime config change is applied during an active conversation? The change must not disrupt the in-flight interaction — it applies to the *next* request.
- What happens when the gateway feature is disabled but code elsewhere references gateway types? Compilation must succeed with no dead-code warnings related to gateway absence.
- What happens when the streaming consumer drops (e.g., UI dismissed) mid-stream? The agent should detect the dropped receiver and cancel or complete the request gracefully without resource leaks.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The gateway module MUST be gated behind an optional compile-time feature flag so it can be excluded from builds targeting constrained environments (Android, embedded).
- **FR-002**: ZeroClaw MUST compile and run on Android targets (aarch64-linux-android, armv7-linux-androideabi) and iOS targets (aarch64-apple-ios) when the gateway feature is disabled.
- **FR-003**: The system MUST expose a programmatic conversation interface that accepts user messages and delivers streamed response events via a push-based `StreamSink<StreamEvent>` pattern (compatible with flutter_rust_bridge FRB 2.x code generation for automatic translation to Dart Streams), without requiring any HTTP/WebSocket server. FRB compatibility is validated by ensuring all public API types in `src/api/` use concrete, `pub`, FRB-translatable signatures — no `dyn Trait`, no `impl Trait` return types, no raw pointers.
- **FR-004**: Streamed response events MUST include at minimum: text chunks, tool call notifications, tool result notifications, and a completion signal.
- **FR-005**: The system MUST expose a programmatic configuration interface that allows reading and updating configuration values at runtime.
- **FR-006**: Runtime configuration changes MUST take effect for the next interaction without requiring an agent restart.
- **FR-007**: Runtime configuration changes MUST be validated before application; invalid changes MUST be rejected with clear error descriptions.
- **FR-008**: The system MUST support loading configuration from a TOML file at startup, using the existing config schema.
- **FR-009**: The system MUST support triggering a config file reload at runtime, merging file values with the current in-memory configuration.
- **FR-010**: When the gateway feature is enabled, all existing gateway functionality (HTTP endpoints, WebSocket chat, SSE events, webhook handlers, pairing, static dashboard) MUST continue to work without regressions.
- **FR-011**: When the gateway feature is disabled, the CLI MUST gracefully report that gateway commands are unavailable rather than failing silently or panicking.
- **FR-012**: The streaming interface MUST support cancellation — if the consumer stops reading, in-flight processing should be cancellable.
- **FR-013**: Configuration changes that affect active subsystems (providers, channels) MUST trigger subsystem reinitialization without requiring host app action or emitting error events. In-flight requests MUST complete using the previous configuration; the new configuration applies starting from the next request only.
- **FR-014**: The system MUST expose a separate observer callback/listener interface that the host app can register at initialization to receive system-wide observability events (LLM requests, tool calls, errors) without requiring the gateway.
- **FR-015**: On Android (library mode), secrets (API keys, tokens) MUST be injectable at runtime via the programmatic config interface rather than read from the config file. The library MUST NOT require secrets to be stored in the TOML file; the host app is responsible for secure storage (e.g., Android Keystore).

### Key Entities

- **ConversationStream**: A Rust async Stream of typed events (chunks, tool calls, tool results, done) produced by the agent in response to a user message. Designed to be consumed via flutter_rust_bridge (FRB), which automatically translates Rust async Streams into Dart Streams for Flutter UI consumption. The primary output surface for the library interface.
- **RuntimeConfig**: The live, mutable configuration state of a running ZeroClaw instance. Supports reads, validated writes, and change notifications.
- **ConfigSource**: The origin of configuration values — file-based (TOML) or programmatic (interface call). Both sources feed into the same RuntimeConfig with merge semantics.
- **FeatureGate**: A compile-time flag (`gateway`) that controls inclusion/exclusion of the HTTP/WebSocket server and its dependencies.
- **ObserverCallback**: A host-registered listener interface that receives system-wide observability events (LLM requests, tool calls, errors). Leverages the existing `Observer` trait. Separate from ConversationStream — provides global visibility independent of active conversations.

## Assumptions

- Android integration will use ZeroClaw as a native Rust library bridged to Dart/Flutter via flutter_rust_bridge (FRB). FRB handles code generation for the FFI boundary, translating Rust async Streams into Dart Streams and Rust structs into Dart classes automatically. The Dart/Flutter wrapper layer is out of scope for this spec — this spec defines the Rust-side interface contract that FRB will consume.
- The Android NDK and iOS (Xcode) toolchains and cross-compilation setup are prerequisites managed by the integrating developer, not by ZeroClaw itself.
- Existing channels (Telegram, Discord, Slack) that depend on network connectivity will still function on Android if the device has network access and those features are compiled in.
- The config file format remains TOML, consistent with the existing config schema. No new config format is introduced.
- On Android, API keys and secrets are expected to be provided at runtime via the programmatic config interface, not stored in the TOML config file. The host Flutter app is responsible for secure secret storage (e.g., Android Keystore, flutter_secure_storage). The Rust library does not implement platform-specific key storage.
- The streaming event protocol mirrors the existing WebSocket chat protocol semantics (chunk/tool_call/tool_result/done), ensuring consistency between the gateway WebSocket interface and the library interface.
- Performance on Android (ARM devices) is expected to be lower than desktop/server — no specific performance targets are set beyond "responsive for interactive chat."

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: ZeroClaw compiles successfully for aarch64-linux-android and aarch64-apple-ios with the gateway feature disabled, producing a functional shared library for each platform.
- **SC-002**: A conversation round-trip (send message → receive full streamed response) completes through the library interface without any HTTP server running.
- **SC-003**: Runtime configuration changes applied via the programmatic interface take effect within the same session — no restart required, verified by behavioral change on next request.
- **SC-004**: Config file reload merges new values into the running instance within 1 second of the reload trigger.
- **SC-005**: Binary size with gateway disabled is at least 20% smaller than the full build (measured for the Android target).
- **SC-006**: All existing gateway integration tests pass when the gateway feature is enabled — zero regressions.
- **SC-007**: The streaming interface delivers the first text chunk to the consumer within 500ms of model response start (no buffering delay beyond network/model latency).
- **SC-008**: 100% of existing CLI commands (except `gateway`) work identically regardless of whether the gateway feature is enabled or disabled.

## Clarifications

### Session 2026-03-19

- Q: How should observability be exposed when gateway is disabled (Android library mode)? → A: Separate observer callback/listener interface registered at initialization (Option B), leveraging the existing Observer trait.
- Q: What happens to in-flight requests during subsystem reinitialization from a config change? → A: In-flight requests complete with the old config; new config applies starting from the next request only (Option A).
- Q: How should the streaming conversation API be designed for FFI/JNI consumption? → A: Rust async Stream via flutter_rust_bridge (FRB) — FRB natively translates Rust async Streams into Dart Streams, so the Rust API uses standard async Stream types and FRB handles the bridging automatically.
- Q: How should API keys/secrets be protected on Android? → A: Secrets must be provided at runtime via the programmatic config interface; the host Flutter app handles secure storage (e.g., Android Keystore). The TOML config file should not contain secrets on Android (Option B).
- Q: Should the spec scope Android only, or include iOS as well? → A: Both Android and iOS are explicit targets (Option B). The Rust library must compile for aarch64-linux-android and aarch64-apple-ios. Flutter/FRB is inherently cross-platform, so supporting both costs minimal extra effort.
