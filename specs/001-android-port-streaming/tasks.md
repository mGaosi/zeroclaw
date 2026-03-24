# Tasks: Mobile Port (Android + iOS) with Optional Gateway & Streaming Interface

**Input**: Design documents from `/specs/001-android-port-streaming/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/library-api.md ✅, quickstart.md ✅

**Tests**: Tests for public API surface are MANDATORY per constitution Principle IV. Every phase that introduces public API functions MUST include a corresponding test task.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

**Implementation Status**: The `src/api/` module (types, conversation, config, lifecycle, observer) is **fully implemented** (1,319 lines, 23+ inline unit tests). The gateway feature flag is partially applied (14 of 17 coupling points gated). `turn_streaming()` exists at `src/agent/agent.rs` L808-1045. Tasks below cover the **remaining** work: gateway `#[cfg]` fixes, integration tests, and cross-compilation validation.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify current implementation state and establish test infrastructure

- [x] T001 Verify all `src/api/` module files compile cleanly with `cargo check --all-targets` and confirm 23+ inline tests pass via `cargo test api`
- [x] T002 [P] Create integration test scaffold in `tests/integration/api_tests.rs` with shared test helpers (mock provider, test config builder, test agent factory) for reuse across US test phases

---

## Phase 2: Foundational — Gateway Feature Flag Fixes (Blocking Prerequisites)

**Purpose**: Complete the gateway feature gating so all user stories that depend on building without gateway can proceed

**⚠️ CRITICAL**: US1, US3, US4 depend on this phase. The 3 remaining `#[cfg]` guards must be applied before non-gateway builds work.

- [x] T003 Add `#[cfg(feature = "gateway")]` attribute on the `Commands::Gateway` enum variant in `src/main.rs` (around L249) so the variant is excluded when gateway is disabled (R-001 fix #1)
- [x] T004 Add `#[cfg(feature = "gateway")]` attribute on the `Commands::Gateway` match arm in `src/main.rs` (around L976) so the dispatch is excluded when gateway is disabled (R-001 fix #2)
- [x] T005 Add graceful CLI error message when gateway feature is disabled: add a `--gateway` or `gateway` subcommand handler that prints "Gateway feature is not available in this build. Recompile with `--features gateway` to enable." when the feature is off — in `src/main.rs` (FR-011)
- [x] T006 Verify `cargo check --no-default-features` compiles successfully with zero errors and no dead-code warnings related to gateway absence (R-001 fix #3 validation)

**Checkpoint**: ZeroClaw compiles cleanly with and without the gateway feature. All user story work can now proceed.

---

## Phase 3: User Story 1 — Android App Embeds ZeroClaw as Library (Priority: P1) 🎯 MVP

**Goal**: Verify ZeroClaw compiles for Android/iOS targets, initializes without gateway, and completes a conversation round-trip via the library interface.

**Independent Test**: Build for aarch64-linux-android with `default-features = false`. Initialize from a test harness. Send a message and receive a streamed response.

### Tests for User Story 1

> **MANDATORY: Constitution Principle IV**

- [x] T007 [P] [US1] Integration test: `init()` with no config file and runtime overrides creates a valid AgentHandle in `tests/integration/api_lifecycle_test.rs` — covers AS-1.1 (FR-002, FR-008)
- [x] T008 [P] [US1] Integration test: `shutdown()` cancels in-flight work and drops resources cleanly in `tests/integration/api_lifecycle_test.rs` — covers lifecycle contract
- [x] T009 [P] [US1] Integration test: `send_message()` round-trip delivers `Chunk` + `Done` events in correct order in `tests/integration/api_conversation_test.rs` — covers AS-1.2 (FR-003, FR-004, SC-002)
- [x] T010 [P] [US1] Integration test: `send_message()` with tool-calling agent delivers `ToolCall` + `ToolResult` events between chunks in `tests/integration/api_conversation_test.rs` — covers AS-1.3 (FR-004)

### Implementation for User Story 1

- [ ] T011 [US1] Add cross-compilation CI check: `cargo check --target aarch64-linux-android --no-default-features` in `.github/workflows/` or verify manually and document in `dev/ci/` — covers SC-001 (FR-002)
- [ ] T012 [US1] Verify `cargo check --target aarch64-apple-ios --no-default-features` compiles (requires Xcode toolchain — document expected setup in quickstart.md if not already covered) — covers SC-001

**Checkpoint**: US1 independently verified — library interface works on Android target without gateway.

---

## Phase 4: User Story 2 — Runtime Configuration via Interface (Priority: P1)

**Goal**: Verify runtime config changes take effect without restart, including provider switching and invalid value rejection.

**Independent Test**: Init with provider A. Call `update_config()` to switch to provider B. Verify next `send_message()` uses provider B.

### Tests for User Story 2

> **MANDATORY: Constitution Principle IV**

- [x] T013 [P] [US2] Integration test: `update_config()` with valid patch changes provider for next interaction in `tests/integration/api_config_test.rs` — covers AS-2.1 (FR-005, FR-006)
- [x] T014 [P] [US2] Integration test: `update_config()` with invalid values returns `ApiError::ValidationError` and preserves previous config in `tests/integration/api_config_test.rs` — covers AS-2.3 (FR-007)
- [x] T015 [P] [US2] Integration test: `update_config()` during in-flight `send_message()` does not affect the in-flight request; new config applies on next request only in `tests/integration/api_config_test.rs` — covers FR-013 edge case
- [x] T016 [P] [US2] Integration test: `get_config()` returns JSON reflecting the latest `update_config()` changes in `tests/integration/api_config_test.rs` — covers FR-005

### Implementation for User Story 2

- [x] T017 [US2] Wire `RuntimeConfigManager::subscribe()` into agent's provider reinitialization: verify in `src/api/conversation.rs` that `apply_config_changes_if_needed()` correctly rebuilds the provider when config changes — covers FR-013 subsystem reinit

**Checkpoint**: US2 independently verified — config changes take effect live.

---

## Phase 5: User Story 3 — Configuration File Loading on Android (Priority: P2)

**Goal**: Verify config file loading at startup and runtime reload with merge semantics that preserve runtime-injected secrets.

**Independent Test**: Init with a TOML config file. Modify the file and trigger `reload_config_from_file()`. Verify new values are active while runtime-injected secrets survive.

### Tests for User Story 3

> **MANDATORY: Constitution Principle IV**

- [x] T018 [P] [US3] Integration test: `init()` with valid config file path loads settings from TOML in `tests/integration/api_config_file_test.rs` — covers AS-3.1 (FR-008)
- [x] T019 [P] [US3] Integration test: `reload_config_from_file()` merges file values with in-memory config, preserving runtime-injected API key in `tests/integration/api_config_file_test.rs` — covers AS-3.2 (FR-009, FR-015, R-008)
- [x] T020 [P] [US3] Integration test: `reload_config_from_file()` with invalid TOML returns error and keeps current config unchanged in `tests/integration/api_config_file_test.rs` — covers AS-3.3
- [x] T021 [P] [US3] Integration test: `init()` with non-existent config file path succeeds with defaults and logs warning in `tests/integration/api_config_file_test.rs` — covers edge case (startup resilience)

### Implementation for User Story 3

> No new implementation needed — `RuntimeConfigManager::reload_from_file()` is complete. Tasks above validate behavior.

**Checkpoint**: US3 independently verified — config file loading and merge semantics work correctly.

---

## Phase 6: User Story 4 — Gateway as Optional Feature (Priority: P2)

**Goal**: Verify gateway feature flag correctly includes/excludes the gateway module, with no regressions when enabled and clean builds when disabled.

**Independent Test**: Build twice — with `--features gateway` and without. Verify gateway endpoints work when enabled. Verify clean compilation and helpful CLI error when disabled.

### Tests for User Story 4

> **MANDATORY: Constitution Principle IV**

- [x] T022 [P] [US4] Integration test: build with `--features gateway` and verify existing gateway integration tests still pass in `tests/` — covers AS-4.1 (FR-010, SC-006)
- [x] T023 [P] [US4] Test: `cargo build --no-default-features` produces binary that outputs graceful error for gateway CLI commands in `tests/integration/api_gateway_feature_test.rs` — covers AS-4.3 (FR-011)
- [x] T024 [P] [US4] Test: compare binary sizes `--features gateway` vs `--no-default-features` and assert ≥20% reduction in `tests/integration/api_gateway_feature_test.rs` or a CI script — covers AS-4.2 (SC-005)

### Implementation for User Story 4

> Gateway `#[cfg]` fixes are in Phase 2 (T003-T005). No additional implementation needed here — tests validate the fixes.

**Checkpoint**: US4 independently verified — gateway is cleanly optional with no regressions.

---

## Phase 7: User Story 5 — Streaming Conversation Events from Library Interface (Priority: P2)

**Goal**: Verify the streaming interface delivers fine-grained typed events (chunks, tool calls, tool results, completion) in correct order with cancellation support.

**Independent Test**: Send a message that triggers a tool call. Verify the stream delivers: text chunks, a `ToolCall` event, a `ToolResult` event, and a `Done` event — all in order.

### Tests for User Story 5

> **MANDATORY: Constitution Principle IV**

- [x] T025 [P] [US5] Integration test: streaming event ordering — `Chunk* → Done` for text-only response in `tests/integration/api_streaming_test.rs` — covers AS-5.1 (FR-003, FR-004)
- [x] T026 [P] [US5] Integration test: streaming event ordering with tool calls — `Chunk* → ToolCall → ToolResult → Chunk* → Done` in `tests/integration/api_streaming_test.rs` — covers AS-5.2 (FR-004)
- [x] T027 [P] [US5] Integration test: `Done` event contains full aggregated response text matching all prior chunks in `tests/integration/api_streaming_test.rs` — covers AS-5.3
- [x] T028 [P] [US5] Integration test: `cancel_message()` during streaming produces `Error` event and stops further events in `tests/integration/api_streaming_test.rs` — covers FR-012
- [x] T029 [P] [US5] Integration test: dropped receiver (simulating dismissed UI) does not leak resources or panic in `tests/integration/api_streaming_test.rs` — covers edge case (resource cleanup)
- [x] T030 [P] [US5] Integration test: non-streaming provider fallback emits single `Chunk` + `Done` in `tests/integration/api_streaming_test.rs` — covers R-009

### Implementation for User Story 5

> No new implementation needed — `send_message()` and `turn_streaming()` are complete. Tasks above validate all streaming event contract guarantees.

**Checkpoint**: US5 independently verified — streaming events are correctly ordered and complete.

---

## Phase 8: Observer Callback Interface (Priority: P2, cross-cutting US2+US5)

**Goal**: Verify the observer callback interface delivers system-wide observability events independently of conversation streams.

**Independent Test**: Register an observer, send a message, verify observer receives LLM request/response events separate from conversation stream events.

### Tests

> **MANDATORY: Constitution Principle IV**

- [x] T031 [P] [US2] Integration test: `register_observer()` receives `LlmRequest` and `LlmResponse` events during `send_message()` in `tests/integration/api_observer_test.rs` — covers FR-014
- [x] T032 [P] [US2] Integration test: `unregister_observer()` stops event delivery immediately in `tests/integration/api_observer_test.rs` — covers FR-014
- [x] T033 [P] [US2] Integration test: multiple observers receive events independently in `tests/integration/api_observer_test.rs` — covers FR-014 multi-consumer

**Checkpoint**: Observer interface verified — host app receives system events without gateway.

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Final validation, documentation, and CI integration

- [x] T034 [P] Run full `cargo test` suite and verify all new integration tests pass alongside existing tests
- [x] T035 [P] Run `cargo clippy --all-targets -- -D warnings` and fix any new warnings introduced by feature flag changes
- [x] T036 [P] Run `cargo fmt --all -- --check` and fix any formatting issues
- [ ] T037 [P] Run quickstart.md scenario manually: verify documented Flutter integration steps are accurate against current `src/api/` implementation
- [ ] T038 Verify all 8 success criteria (SC-001 through SC-008) are covered by at least one task — add missing coverage if needed
- [ ] T039 Run `./dev/ci.sh all` for full CI validation (HIGH risk changes: gateway feature flag touches `src/main.rs`, `src/tools/`)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies — can start immediately
- **Phase 2 (Foundational)**: Depends on Phase 1 — **BLOCKS** US1, US3, US4 (gateway feature fixes required)
- **Phase 3 (US1 - P1)**: Depends on Phase 2 — core library embedding
- **Phase 4 (US2 - P1)**: Depends on Phase 1 only — runtime config is independent of gateway fixes
- **Phase 5 (US3 - P2)**: Depends on Phase 2 — config file loading needs clean no-gateway build
- **Phase 6 (US4 - P2)**: Depends on Phase 2 — gateway feature validation
- **Phase 7 (US5 - P2)**: Depends on Phase 1 only — streaming tests independent of gateway
- **Phase 8 (Observer)**: Depends on Phase 1 only — observer tests independent of gateway
- **Phase 9 (Polish)**: Depends on all prior phases

### User Story Dependencies

```
Phase 1 (Setup)
  │
  ├──→ Phase 2 (Foundational: gateway #[cfg] fixes) ──BLOCKS──┐
  │         │                                                    │
  │         ├──→ Phase 3 (US1: Android library) ────────────────→│
  │         ├──→ Phase 5 (US3: Config file loading) ────────────→│
  │         └──→ Phase 6 (US4: Gateway optional) ──────────────→│
  │                                                              │
  ├──→ Phase 4 (US2: Runtime config) ──────────────────────────→│
  ├──→ Phase 7 (US5: Streaming events) ────────────────────────→│
  ├──→ Phase 8 (Observer callbacks) ───────────────────────────→│
  │                                                              │
  └──────────────────────────────────────────────────────────────→ Phase 9 (Polish)
```

### Within Each User Story

- Integration tests can run in parallel (different test files, marked [P])
- Implementation tasks within a phase run sequentially
- Story complete = all its tests pass

### Parallel Opportunities

**After Phase 1 completes**:
- Phase 4 (US2), Phase 7 (US5), Phase 8 (Observer) can all start in parallel — they don't depend on gateway fixes

**After Phase 2 completes**:
- Phase 3 (US1), Phase 5 (US3), Phase 6 (US4) can start in parallel

**Within any test phase**:
- All tasks marked [P] can run in parallel (separate test files)

---

## Parallel Example: Post-Phase-2 Execution

```bash
# After Phase 2 foundational fixes are applied, launch all US test phases together:

# Worker A (US1):
Task T007: "Integration test: init() with no config file..."       # api_lifecycle_test.rs
Task T009: "Integration test: send_message() round-trip..."        # api_conversation_test.rs
Task T011: "Cross-compilation CI check..."                         # CI config

# Worker B (US2):
Task T013: "Integration test: update_config() valid patch..."      # api_config_test.rs
Task T014: "Integration test: update_config() invalid values..."   # api_config_test.rs
Task T017: "Wire config subscribe into provider reinit..."         # conversation.rs

# Worker C (US4 + US5):
Task T022: "Gateway integration tests still pass..."               # existing tests
Task T025: "Streaming event ordering test..."                      # api_streaming_test.rs
Task T031: "Observer receives LLM events..."                       # api_observer_test.rs
```

---

## Implementation Strategy

### MVP First (User Stories 1 + 2 Only)

1. Complete Phase 1: Setup — verify existing implementation
2. Complete Phase 2: Foundational gateway `#[cfg]` fixes
3. Complete Phase 3: US1 — Android library integration tests
4. Complete Phase 4: US2 — Runtime config integration tests
5. **STOP and VALIDATE**: Test US1 + US2 independently → functional MVP

### Incremental Delivery

1. Setup + Foundational → Gateway builds cleanly both ways
2. US1 → Android library works → MVP core ✅
3. US2 → Runtime config works → MVP usable ✅
4. US3 → Config file reload → Deployment flexibility
5. US4 → Gateway regression tests → Server users unaffected
6. US5 → Streaming event validation → Professional-grade streaming
7. Observer → System visibility → Production observability
8. Polish → CI green, all SC validated

### Parallel Team Strategy

With 3 workers after Phase 2:
- **Worker A**: US1 (P1) + US3 (P2) — both involve cross-compilation / config file
- **Worker B**: US2 (P1) + Observer (P2) — both involve runtime config interactions
- **Worker C**: US4 (P2) + US5 (P2) + Polish — gateway feature + streaming validation

---

## Notes

- [P] tasks = different files, no dependencies on incomplete tasks
- [Story] label maps task to specific user story for traceability
- Most implementation is **already complete** — tasks focus on integration tests and 3 remaining `#[cfg]` fixes
- Inline unit tests (23+ in `src/api/`) are NOT duplicated — integration tests cover cross-module behavior
- Constitution Principle IV: every public API function has at least one integration test
- All edge cases from spec.md are mapped to specific test tasks (concurrent calls, dropped receiver, non-streaming provider, invalid config, missing file)
