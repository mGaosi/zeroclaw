# Tasks: Mobile Port (Android + iOS) with Optional Gateway & Streaming Interface

**Input**: Design documents from `specs/001-android-port-streaming/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/library-api.md, quickstart.md

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story this task belongs to (US1–US5)
- All file paths are relative to repository root

---

## Phase 1: Setup

**Purpose**: New module scaffolding and shared types needed by all user stories

- [x] T001 Create `src/api/mod.rs` with public re-exports for StreamEvent, ApiError, ConfigPatch, ObserverEventDto, AgentHandle
- [x] T002 [P] Define `StreamEvent` enum (Chunk, ToolCall, ToolResult, Done, Error) in `src/api/types.rs`
- [x] T003 [P] Define `ApiError` enum (ValidationError, NotInitialized, Internal, ConfigFileError) in `src/api/types.rs`
- [x] T004 [P] Define `ConfigPatch` struct with all optional fields and validation logic in `src/api/types.rs`
- [x] T005 [P] Define `ObserverEventDto` enum (FRB-compatible subset of ObserverEvent) in `src/api/types.rs`
- [x] T006 Add `pub mod api;` to `src/lib.rs`

**Checkpoint**: Shared types compile. `cargo check` passes.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Gateway feature-gating — MUST complete before US1/US2 can be validated on a no-gateway build. Also establishes RuntimeConfigManager and ObserverCallbackRegistry used by multiple stories.

**⚠️ CRITICAL**: US1 requires building without gateway. US2/US3 depend on RuntimeConfigManager. All user stories depend on this phase.

- [x] T007 Add `gateway` feature flag to `Cargo.toml`: define the feature, make axum/tower/tower-http/rust-embed/mime_guess/http-body-util optional deps, **append** `gateway` to the existing `default` features list (preserving `observability-prometheus`, `channel-nostr`, `skill-creation`)
- [x] T008 Gate `pub mod gateway;` in `src/lib.rs` behind `#[cfg(feature = "gateway")]`
- [x] T009 [P] Gate `GatewayCommands` enum and `Commands::Gateway` variant in `src/lib.rs` behind `#[cfg(feature = "gateway")]`
- [x] T010 [P] Gate gateway CLI command dispatch (match arm) in `src/main.rs` behind `#[cfg(feature = "gateway")]`; add fallback arm that prints user-friendly error: "The gateway feature is not enabled in this build. Recompile with `--features gateway` to use gateway commands."
- [x] T011 [P] Gate gateway spawn block in `src/daemon/mod.rs` behind `#[cfg(feature = "gateway")]`
- [x] T012 [P] Gate `NodeRegistry`/`NodeInvocation` import and node tool registration in `src/tools/node_tool.rs` behind `#[cfg(feature = "gateway")]`
- [x] T013 [P] Gate gateway-related test imports in `tests/` (component/gateway.rs, component/whatsapp_webhook_security.rs) behind `#[cfg(feature = "gateway")]`
- [x] T014 Implement `RuntimeConfigManager` struct in `src/api/config.rs` with `config: Arc<Mutex<Config>>`, `tx: watch::Sender<Config>`, `config_path: Option<PathBuf>`, and methods: `new()`, `get_config()`, `subscribe()`
- [x] T015 [P] Implement `ObserverCallbackRegistry` struct in `src/api/observer.rs` with `register()`, `unregister()`, and `Observer` trait implementation that forwards events as `ObserverEventDto`
- [x] T016 Implement `AgentHandle` struct in `src/api/lifecycle.rs` with fields: `agent: Arc<Mutex<Agent>>`, `config_manager: Arc<RuntimeConfigManager>`, `observer_registry: Arc<ObserverCallbackRegistry>`, `cancel_token: CancellationToken`
- [x] T017 Verify `cargo check --all-targets` passes with default features (gateway enabled)
- [x] T018 Verify `cargo check --all-targets --no-default-features` passes (gateway disabled, no compilation errors)

**Checkpoint**: Gateway is optional. Core infrastructure types exist. Both `--features gateway` and `--no-default-features` compile cleanly.

---

## Phase 3: User Story 1 — Android App Embeds ZeroClaw as Library (Priority: P1) 🎯 MVP

**Goal**: Host app can initialize ZeroClaw without gateway, send a message, and receive a streamed response.

**Independent Test**: Build with `--no-default-features`. Call `init()` → `send_message()` → receive `StreamEvent::Chunk` + `StreamEvent::Done`.

### Implementation for User Story 1

- [x] T019 [US1] Implement `init()` in `src/api/lifecycle.rs`: load config from optional path, apply `ConfigPatch` overrides, build `Agent` via `AgentBuilder`, construct `AgentHandle`
- [x] T020 [US1] Implement `shutdown()` in `src/api/lifecycle.rs`: trigger `CancellationToken`, drop agent, flush observer events
- [x] T021 [US1] Implement `Agent::turn_streaming()` in `src/agent/agent.rs`: same tool loop as `turn()` but accepts `mpsc::Sender<StreamEvent>` and emits Chunk events for text and Done event at completion
- [x] T022 [US1] Implement `send_message()` in `src/api/conversation.rs`: accept `StreamSink<StreamEvent>` (FRB push-based sink), internally create `mpsc::channel`, spawn async task that calls `agent.turn_streaming(mpsc_tx)` and forwards received events to the `StreamSink`, return immediately
- [x] T023 [US1] Implement `cancel_message()` in `src/api/conversation.rs`: trigger `CancellationToken` to abort in-flight `turn_streaming()`, send `StreamEvent::Error` with cancellation message
- [x] T024 [US1] Wire cancellation detection in `Agent::turn_streaming()`: check `CancellationToken` between tool loop iterations and before provider calls; on cancellation, emit `StreamEvent::Error` and return early

### Tests for User Story 1

- [x] T056 [P] [US1] Write unit test for `turn_streaming()` in `tests/api_streaming_test.rs`: verify Chunk→Done event sequence for a simple text response (mock provider)
- [x] T057 [P] [US1] Write unit test for `turn_streaming()` cancellation in `tests/api_streaming_test.rs`: trigger CancellationToken mid-stream, verify Error event received and task completes
- [x] T058 [P] [US1] Write integration test for `init()` → `send_message()` → `shutdown()` lifecycle in `tests/api_streaming_test.rs`: verify no panics, no resource leaks

**Checkpoint**: `init()` → `send_message()` → receive `Chunk` + `Done`. `cancel_message()` aborts cleanly. No HTTP server started. Tests pass.

---

## Phase 4: User Story 2 — Runtime Configuration via Interface (Priority: P1) 🎯 MVP

**Goal**: Host app can read current config, apply partial updates at runtime, changes take effect on next interaction.

**Independent Test**: `init()` with provider A → `update_config(provider: B)` → `send_message()` → verify response uses provider B.

### Implementation for User Story 2

- [x] T025 [US2] Implement `ConfigPatch::apply_to(config: &mut Config)` merge logic in `src/api/types.rs`: for each `Some` field, overwrite the corresponding `Config` field
- [x] T026 [US2] Implement `RuntimeConfigManager::update_config(patch)` in `src/api/config.rs`: validate patch → apply to config clone → run `Config::validate()` → if valid: swap config, save to disk (if path), send watch notification → if invalid: return `ApiError::ValidationError`
- [x] T027 [US2] Implement `get_config()` public API function in `src/api/config.rs`: serialize current `Config` to JSON string via `serde_json`
- [x] T028 [US2] Implement `update_config()` public API function in `src/api/config.rs`: delegate to `RuntimeConfigManager::update_config()`
- [x] T029 [US2] Wire agent to subscribe to config changes: in `AgentHandle` initialization, the agent holds a `watch::Receiver<Config>` and re-creates the provider on config change before the next `turn_streaming()` call
- [x] T030 [US2] Ensure in-flight request isolation: `turn_streaming()` captures `Arc<dyn Provider>` at start of the call — config changes during processing do not affect the current turn (FR-013)

### Tests for User Story 2

- [x] T059 [P] [US2] Write unit test for `ConfigPatch::apply_to()` in `tests/api_config_test.rs`: verify partial merge (only `Some` fields applied)
- [x] T060 [P] [US2] Write unit test for `update_config()` validation rejection in `tests/api_config_test.rs`: invalid temperature, empty api_key, zero max_tool_iterations all return `ApiError::ValidationError`
- [x] T061 [P] [US2] Write unit test for in-flight isolation in `tests/api_config_test.rs`: start `turn_streaming()`, apply config change mid-turn, verify turn completes with old config (FR-013)
- [x] T062 [P] [US2] Write test for FR-015 (secrets injection) in `tests/api_config_test.rs`: call `init()` with no `api_key` in TOML config, provide `api_key` via `ConfigPatch` overrides, verify agent initializes successfully

**Checkpoint**: `get_config()` returns JSON. `update_config()` with valid patch succeeds and affects next interaction. Invalid patch returns `ApiError::ValidationError`. In-flight requests unaffected. Tests pass.

---

## Phase 5: User Story 3 — Configuration File Loading on Android (Priority: P2)

**Goal**: ZeroClaw loads config from a TOML file at startup and supports triggered file reload at runtime.

**Independent Test**: `init(config_path: Some("test.toml"))` loads file. Modify file → `reload_config_from_file()` → verify new values active.

**Depends on**: US2 (RuntimeConfigManager must exist)

### Implementation for User Story 3

- [x] T031 [US3] Implement `RuntimeConfigManager::reload_from_file()` in `src/api/config.rs`: read TOML file → parse → validate → merge with current config → send watch notification → on error: return `ApiError::ConfigFileError`, keep previous config
- [x] T032 [US3] Implement `reload_config_from_file()` public API function in `src/api/config.rs`: validate that `config_path` was set at init, delegate to `RuntimeConfigManager::reload_from_file()`
- [x] T033 [US3] Handle missing config file gracefully in `init()`: if `config_path` is `Some` but file doesn't exist, log warning and continue with defaults (don't panic)

**Checkpoint**: File-based config loads at startup. `reload_config_from_file()` merges new values. Missing/invalid file returns error without crashing.

---

## Phase 6: User Story 4 — Gateway as Optional Feature (Priority: P2)

**Goal**: Existing gateway functionality works unchanged with `--features gateway`. Without the feature, binary is smaller and no HTTP deps are linked.

**Independent Test**: Build with `--features gateway` → run gateway command → all endpoints respond. Build without → gateway command prints error. Compare binary sizes.

**Note**: The `#[cfg]` guards were already applied in Phase 2 (T007–T013). This phase validates the full behavior and handles edge cases.

### Implementation for User Story 4

- [x] T034 [US4] Verify all existing gateway integration tests pass with `cargo test --features gateway` in `tests/component/gateway.rs`
- [x] T035 [US4] Verify `cargo test --no-default-features` passes — no gateway test compilation errors
- [x] T037 [US4] Audit `src/commands/self_test.rs` and `src/doctor/mod.rs` for gateway port references — gate any gateway-specific health checks behind `#[cfg(feature = "gateway")]`
- [x] T038 [US4] Verify no dead-code warnings related to gateway absence when compiling with `--no-default-features` — suppress or fix any warnings
- [x] T063 [US4] Measure binary size: build for aarch64-linux-android with and without `gateway` feature, verify ≥20% size reduction (SC-005)
- [x] T064 [P] [US4] Run CLI smoke-test without gateway: execute `zeroclaw --help`, `zeroclaw doctor`, `zeroclaw self-test` with `--no-default-features` build, verify all succeed except gateway commands (SC-008)

**Checkpoint**: `--features gateway` build has zero regressions. `--no-default-features` compiles cleanly with no warnings. Gateway CLI command shows helpful error when feature is disabled.

---

## Phase 7: User Story 5 — Streaming Conversation Events (Priority: P2)

**Goal**: Stream delivers rich event types: Chunk, ToolCall, ToolResult, Done — not just text.

**Independent Test**: Send a message that triggers a tool call. Verify stream delivers `Chunk*` → `ToolCall` → `ToolResult` → `Chunk*` → `Done` in order.

**Depends on**: US1 (basic streaming must work first)

### Implementation for User Story 5

- [x] T039 [US5] Extend `Agent::turn_streaming()` in `src/agent/agent.rs` to emit `StreamEvent::ToolCall { tool, arguments }` before each tool execution
- [x] T040 [US5] Extend `Agent::turn_streaming()` in `src/agent/agent.rs` to emit `StreamEvent::ToolResult { tool, output, success }` after each tool execution
- [x] T041 [US5] Wire provider-level streaming: when `provider.stream_chat_with_history()` is available, forward `StreamChunk` deltas as individual `StreamEvent::Chunk` events instead of waiting for the full response
- [x] T042 [US5] Implement graceful stream consumer drop detection: if `mpsc::Sender::send()` returns `Err` (receiver dropped), abort the tool loop cleanly and log a warning — no resource leaks (FR-012)
- [x] T043 [US5] Ensure `StreamEvent::Done { full_response }` always contains the fully aggregated text from all chunks, matching the return value of `turn()`

### Tests for User Story 5

- [x] T065 [P] [US5] Write unit test for rich streaming events in `tests/api_streaming_test.rs`: send a message that triggers a tool call (mock provider + mock tool), verify stream delivers Chunk→ToolCall→ToolResult→Chunk→Done in order
- [x] T066 [P] [US5] Write unit test for consumer drop in `tests/api_streaming_test.rs`: drop the receiver mid-stream, verify no panic and no resource leak

**Checkpoint**: Stream delivers all event types in correct order. Tool calls emit ToolCall + ToolResult events. Consumer drop triggers clean cancellation. Tests pass.

---

## Phase 8: Observer Callback Interface (FR-014)

**Goal**: Host app can register observer callbacks and receive system-wide observability events without the gateway.

**Independent Test**: Register observer → send message → verify LlmRequest, LlmResponse, ToolCallStart, ToolCallEnd, TurnComplete events received.

### Implementation

- [x] T044 [FR-014] Implement `register_observer()` public API function in `src/api/observer.rs`: accept `StreamSink<ObserverEventDto>`, register in `ObserverCallbackRegistry`, return observer ID
- [x] T045 [FR-014] Implement `unregister_observer()` public API function in `src/api/observer.rs`: remove callback by ID
- [x] T046 [FR-014] Wire `ObserverCallbackRegistry` into agent initialization: register it as an `Observer` in the agent's observer chain so it receives all runtime events
- [x] T047 [FR-014] Implement `ObserverEvent` → `ObserverEventDto` conversion: map Duration to u64 millis, usize to u32, filter internal-only events (HeartbeatTick, CacheHit, CacheMiss, HandStarted, HandCompleted, HandFailed) and metrics (HandRunDuration, HandFindingsCount, HandSuccessRate)

### Tests for Observer

- [x] T067 [P] [FR-014] Write unit test for observer registration/delivery in `tests/api_observer_test.rs`: register observer, trigger agent turn, verify LlmRequest + LlmResponse + TurnComplete events received
- [x] T068 [P] [FR-014] Write unit test for observer unregistration in `tests/api_observer_test.rs`: unregister observer, trigger agent turn, verify no more events delivered

**Checkpoint**: Host observer receives LlmRequest, LlmResponse, ToolCallStart, ToolCallEnd, TurnComplete events. Unregister stops delivery. Tests pass.

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Final validation, documentation, and cleanup

- [x] T048 [P] Run `cargo clippy --all-targets -- -D warnings` and fix any warnings across all modified files
- [x] T049 [P] Run `cargo clippy --all-targets --no-default-features -- -D warnings` and fix warnings for gateway-disabled build
- [x] T050 [P] Run `cargo fmt --all -- --check` and fix any formatting issues
- [x] T051 Verify `cargo test` passes with default features (all existing tests green)
- [x] T052 Verify `cargo test --no-default-features` passes (no gateway test failures)
- [x] T053 [P] Verify all public API types in `src/api/` have `pub` visibility and FRB-compatible signatures (no `dyn Trait`, no `impl Trait` returns, concrete types only)
- [x] T054 [P] Validate quickstart.md code samples match actual API signatures in `src/api/`
- [x] T069 [P] Verify cross-compilation for all mobile targets: `cargo check --target aarch64-linux-android --no-default-features`, `cargo check --target armv7-linux-androideabi --no-default-features`, `cargo check --target aarch64-apple-ios --no-default-features` (SC-001, FR-002)
- [x] T070 [P] First-chunk latency smoke-test: send a message via `send_message()`, measure wall-clock time from call to first `StreamEvent::Chunk` received, assert ≤500ms above provider latency (SC-007)
- [x] T055 Run full pre-PR validation: `./dev/ci.sh all`

**Checkpoint**: All builds pass. All tests green. Clippy clean. Format clean. API surface is FRB-ready.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies — start immediately
- **Phase 2 (Foundational)**: Depends on Phase 1 — BLOCKS all user stories
- **Phase 3 (US1)**: Depends on Phase 2 — MVP core
- **Phase 4 (US2)**: Depends on Phase 2 — can run in parallel with US1
- **Phase 5 (US3)**: Depends on Phase 4 (US2) — extends RuntimeConfigManager
- **Phase 6 (US4)**: Depends on Phase 2 — can run in parallel with US1/US2
- **Phase 7 (US5)**: Depends on Phase 3 (US1) — extends basic streaming
- **Phase 8 (Observer)**: Depends on Phase 2 — can run in parallel with US1/US2
- **Phase 9 (Polish)**: Depends on all previous phases

### User Story Dependencies

```
Phase 2 (Foundational)
  ├──→ US1 (P1) ──→ US5 (P2) ──┐
  ├──→ US2 (P1) ──→ US3 (P2) ──┤
  ├──→ US4 (P2) ────────────────┤
  └──→ Observer ────────────────┤
                                └──→ Phase 9 (Polish)
```

### Parallel Opportunities

- **Phase 1**: T002, T003, T004, T005 are all [P] (different sections of `src/api/types.rs` or independent files)
- **Phase 2**: T009, T010, T011, T012, T013 are all [P] (different files, independent `#[cfg]` guards)
- **Phase 3 + Phase 4**: US1 and US2 can proceed in parallel after Phase 2
- **Phase 5 + Phase 6 + Phase 7**: US3, US4, US5 can proceed in parallel (after their respective dependencies)
- **Phase 9**: T048, T049, T050, T053, T054 are all [P]

---

## Parallel Example: After Phase 2 Completes

```
# Worker A: User Story 1 (core streaming)
Task T019: Implement init() in src/api/lifecycle.rs
Task T020: Implement shutdown() in src/api/lifecycle.rs
Task T021: Implement Agent::turn_streaming() in src/agent/agent.rs
Task T022: Implement send_message() in src/api/conversation.rs

# Worker B: User Story 2 (runtime config) — simultaneously
Task T025: Implement ConfigPatch::apply_to() in src/api/types.rs
Task T026: Implement RuntimeConfigManager::update_config() in src/api/config.rs
Task T027: Implement get_config() API in src/api/config.rs
Task T028: Implement update_config() API in src/api/config.rs

# Worker C: User Story 4 (gateway validation) — simultaneously
Task T034: Verify gateway tests pass with --features gateway
Task T037: Audit self_test/doctor for gateway refs
```

---

## Implementation Strategy

### MVP First (User Stories 1 + 2)

1. Complete Phase 1: Setup (shared types)
2. Complete Phase 2: Foundational (feature flag + core infra)
3. Complete Phase 3: US1 (init + streaming) — **basic mobile functionality works**
4. Complete Phase 4: US2 (runtime config) — **config is changeable without restart**
5. **STOP and VALIDATE**: Can init, stream messages, update config — all without gateway

### Incremental Delivery

1. Setup + Foundational → Build compiles with/without gateway
2. US1 → init + send_message + streaming works → **MVP Demo**
3. US2 → runtime config update works → **MVP Complete**
4. US3 → file-based config + reload works
5. US4 → gateway feature fully validated, binary size reduction confirmed
6. US5 → rich streaming events (tool_call, tool_result)
7. Observer → system-wide event delivery to host
8. Polish → CI-clean, FRB-ready

### Suggested MVP Scope

**Phase 1 + Phase 2 + Phase 3 (US1) + Phase 4 (US2)** = ~36 tasks (including tests) → delivers a fully functional embedded agent with streaming + runtime config on Android.

### Scope Notes

- **Android backgrounding** (spec edge case): The host Flutter app is responsible for managing process lifecycle when backgrounded. ZeroClaw's `CancellationToken` mechanism (T023/T024) allows the host to cancel in-flight requests before suspension. No ZeroClaw-side process lifecycle management is needed — this is deferred to the host integration layer.
- **flutter_rust_bridge (FRB)** is an external code generation tool that consumes ZeroClaw's `src/api/` types. It is available as an optional Cargo dependency behind the `frb` feature flag (`flutter_rust_bridge = { version = "2.11", optional = true }`). When `--features frb` is enabled, `StreamSink`-based wrapper functions (`send_message_stream`, `register_observer_stream`) become available for direct FRB consumption. FRB compatibility is validated by ensuring all public API types use concrete, `pub`, FRB-translatable signatures (T053).
