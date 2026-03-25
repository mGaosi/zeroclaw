# Tasks: Host-Side Tool Registration for Flutter

**Input**: Design documents from `/specs/002-host-tool-registration/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/library-api.md ✅, quickstart.md ✅

**Tests**: Tests for public API surface are MANDATORY per constitution Principle IV. Every phase that introduces public API functions MUST include a corresponding test task.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add new module, types, and registry skeleton — no behavioral changes yet

- [x] T001 Add `HostToolSpec`, `ToolRequest`, `ToolResponse` structs to src/api/types.rs (FR-001, FR-004)
- [x] T002 Create src/api/host_tools.rs with `HostToolRegistry` struct, `HostToolMeta` internal type, and `HostToolProxy` struct (all fields, no method impls yet)
- [x] T003 Add `host_tools` module declaration and re-exports to src/api/mod.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core registry logic and Agent mutation method that ALL user stories depend on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T004 Implement `HostToolRegistry::new()` — create mpsc unbounded channel pair, empty tools/pending maps, next_id=1 in src/api/host_tools.rs (FR-005)
- [x] T005 Implement `validate_spec()` — validate name non-empty, description non-empty, parameters_schema parses as JSON object, timeout_seconds > 0 if present in src/api/host_tools.rs (FR-010)
- [x] T006 Implement `HostToolRegistry::register()` — accept `HostToolSpec` + `&[String]` builtin names, call `validate_spec`, check collisions with built-in and existing host tools, parse schema, assign ID, insert into tools map in src/api/host_tools.rs (FR-001, FR-009, FR-010, FR-011)
- [x] T007 [P] Implement `HostToolRegistry::unregister()` — remove by tool_id, return error if not found in src/api/host_tools.rs (FR-002, FR-011)
- [x] T008 [P] Implement `HostToolRegistry::create_proxies()` — snapshot tools, create `Vec<Box<dyn Tool>>` of `HostToolProxy` instances sharing request_tx and pending map in src/api/host_tools.rs (FR-003, FR-008)
- [x] T009 Implement `HostToolProxy` — `impl Tool for HostToolProxy` with `name()`, `description()`, `parameters_schema()`, and `execute()` using oneshot correlation, timeout, and cancellation in src/api/host_tools.rs (FR-004, FR-006, FR-012)
- [x] T010 [P] Implement `HostToolRegistry::take_receiver()` — take mpsc receiver once for `setup_tool_handler` in src/api/host_tools.rs (FR-005)
- [x] T011 Add `builtin_tool_count: usize` field to `Agent` struct, set during `build()`, and implement `Agent::replace_host_tools()` — truncate to builtin_count, append host tools, regenerate tool_specs in src/agent/agent.rs (FR-003, FR-008)
- [x] T012 [P] Implement `Agent::builtin_tool_names()` and `Agent::tool_names()` accessors in src/agent/agent.rs (FR-009)
- [x] T013 Add `host_tool_registry: Arc<HostToolRegistry>` field to `AgentHandle`, initialize in `init()` and `from_agent_for_test()`, add `host_tool_registry()` accessor in src/api/lifecycle.rs (FR-008, FR-013)

### Tests for Foundational Phase

- [x] T014 [P] Unit test: `validate_spec` rejects empty name, empty description, non-JSON schema, non-object schema, zero timeout in src/api/host_tools.rs #[cfg(test)] (FR-010)
- [x] T015 [P] Unit test: `register` rejects duplicate host tool name, rejects built-in tool name collision in src/api/host_tools.rs #[cfg(test)] (FR-009)
- [x] T016 [P] Unit test: `register` happy path returns sequential IDs, `unregister` removes tool, double-unregister returns error in src/api/host_tools.rs #[cfg(test)] (FR-001, FR-002)
- [x] T017 [P] Unit test: `create_proxies` returns correct count and `Tool` trait methods return expected values in src/api/host_tools.rs #[cfg(test)] (FR-003)
- [x] T018 [P] Unit test: `replace_host_tools` on Agent — verify builtin tools preserved, host tools appended, tool_specs regenerated in src/agent/agent.rs #[cfg(test)] (FR-008)
- [x] T019 Unit test: concurrent register/unregister from multiple threads does not panic or corrupt state in src/api/host_tools.rs #[cfg(test)] (FR-011)

**Checkpoint**: Foundation ready — registry, proxy, agent mutation, and validation all tested. User story implementation can now begin.

---

## Phase 3: User Story 1 — Flutter App Registers a Custom Tool (Priority: P1) 🎯 MVP

**Goal**: A host app can register a tool, set up the handler channel, send a message that triggers the tool, handle the request, return a response, and see the result in the agent's reply.

**Independent Test**: Initialize → setup_tool_handler → register_tool → send_message (triggers tool) → handle ToolRequest → submit_tool_response → verify agent response includes tool output.

### Public API Functions for US1

- [x] T020 Implement `setup_tool_handler(handle, sender)` — check init, take receiver, spawn forwarding task in src/api/host_tools.rs (FR-004, FR-005, FR-013)
- [x] T021 Implement `submit_tool_response(handle, response)` — check init, look up pending oneshot sender, send response, log debug if not found in src/api/host_tools.rs (FR-004)
- [x] T022 Implement `register_tool(handle, spec)` — check init, get builtin_tool_names from agent, call registry.register, create_proxies, replace_host_tools on agent in src/api/host_tools.rs (FR-001, FR-003, FR-009, FR-010, FR-013)
- [x] T023 Implement `unregister_tool(handle, tool_id)` — check init, call registry.unregister, create_proxies, replace_host_tools on agent in src/api/host_tools.rs (FR-002)
- [x] T024 Wire host tool re-injection in `apply_config_changes_if_needed` — after `Agent::from_config()`, call `agent.replace_host_tools(registry.create_proxies(...))` in src/api/conversation.rs (FR-008)

### Tests for US1

- [x] T025 [P] [US1] Unit test: `HostToolProxy::execute` round-trip — register tool, create proxy, spawn mock receiver, call execute, submit response, verify ToolResult in src/api/host_tools.rs #[cfg(test)] (FR-004)
- [x] T026 [P] [US1] Unit test: `HostToolProxy::execute` timeout — register tool, create proxy, do NOT submit response, verify timeout error in src/api/host_tools.rs #[cfg(test)] (FR-006)
- [x] T027 [P] [US1] Unit test: `setup_tool_handler` returns error when called before init (NotInitialized) in src/api/host_tools.rs #[cfg(test)] (FR-013)
- [x] T028 [US1] Integration test: register_tool returns error when handle not initialized in tests/integration/api_host_tools_test.rs (FR-013)
- [x] T029 [US1] Integration test: full round-trip — init → setup_tool_handler → register_tool → send_message → handle ToolRequest → submit_tool_response → verify StreamEvent::Done in tests/integration/api_host_tools_test.rs (SC-001)

**Checkpoint**: US1 MVP complete — a single host tool can be registered and invoked end-to-end.

---

## Phase 4: User Story 2 — Dynamic Tool Registration and Unregistration (Priority: P2)

**Goal**: Tools can be registered and unregistered at any time during the app lifecycle. The agent's tool catalog updates dynamically without restart.

**Independent Test**: Register tool A → use it → unregister A → register tool B → use B → verify A is gone and B works.

### Tests for US2

- [x] T030 [P] [US2] Unit test: mid-session dynamic removal — register tool, call create_proxies, unregister tool, call create_proxies again, verify tool absent from second proxy set in src/api/host_tools.rs #[cfg(test)] (FR-002)
- [x] T031 [P] [US2] Unit test: unregister while in-flight invocation — verify in-flight completes normally, removal takes effect for next create_proxies in src/api/host_tools.rs #[cfg(test)]
- [x] T032 [US2] Integration test: dynamic registration round-trip — register A, use A, unregister A, register B, use B in tests/integration/api_host_tools_test.rs (SC-002)

**Checkpoint**: US2 complete — dynamic tool lifecycle verified.

---

## Phase 5: User Story 3 — Host Tool Participates in Streaming Conversation (Priority: P2)

**Goal**: Host tool invocations emit `ToolCall` and `ToolResult` events in the conversation stream, indistinguishable from built-in tool events.

**Independent Test**: Register tool → send message triggering it → verify ToolCall event arrives before ToolResult event in stream, both with correct tool name and data.

### Tests for US3

- [x] T033 [P] [US3] Integration test: streaming events order — verify ToolCall for host tool appears before ToolResult, both contain correct tool name / arguments / output in tests/integration/api_host_tools_test.rs (SC-003, FR-007)

**Checkpoint**: US3 complete — host tools are indistinguishable from built-in tools in streaming.

---

## Phase 6: Edge Cases & Robustness

**Purpose**: Cover all edge cases from the spec — timeout, cancellation, shutdown, rebuild persistence, malformed responses, channel recovery.

- [x] T034 [P] Unit test: `HostToolProxy::execute` cancellation — pass a CancellationToken, cancel it, verify execute returns cancelled error in src/api/host_tools.rs #[cfg(test)] (FR-012)
- [x] T035 [P] Unit test: shutdown while pending — drop registry pending map sender, verify execute returns "channel closed" in src/api/host_tools.rs #[cfg(test)]
- [x] T036 [P] Unit test: submit_tool_response with unknown request_id — verify silently discarded with debug log in src/api/host_tools.rs #[cfg(test)]
- [x] T037 [P] Unit test: malformed/empty ToolResponse — verify treated as tool failure in src/api/host_tools.rs #[cfg(test)]
- [x] T038 Unit test: rebuild persistence — register tools, create proxies, simulate rebuild by calling create_proxies again, verify same tools in new proxy set in src/api/host_tools.rs #[cfg(test)] (FR-008)
- [x] T039 [US2] Integration test: re-registration persistence — register tool, use it, unregister, re-register same name, use again in tests/integration/api_host_tools_test.rs (SC-004)

**Checkpoint**: All edge cases covered — system is robust under failure scenarios.

---

## Phase 7: Channel Re-establishment (FR-014)

**Purpose**: Support re-connectable handler channel for mobile app lifecycle resilience.

- [x] T040 Refactor `HostToolRegistry` — change `request_tx` from fixed sender to `Arc<Mutex<mpsc::UnboundedSender<ToolRequest>>>` and `request_rx` to allow re-creation of channel pair in src/api/host_tools.rs (FR-014)
- [x] T041 Update `HostToolProxy` — read request_tx from shared `Arc<Mutex<>>` instead of owning a clone, so channel swap is transparent to existing proxies in src/api/host_tools.rs (FR-014)
- [x] T042 Implement `HostToolRegistry::reset_channel()` — create fresh mpsc pair, swap request_tx under lock, store new receiver in request_rx in src/api/host_tools.rs (FR-014)
- [x] T043 Update `setup_tool_handler` — if receiver already taken, call `reset_channel()` first, then take new receiver in src/api/host_tools.rs (FR-014)
- [x] T044 Update `create_proxies` — pass shared `Arc<Mutex<UnboundedSender>>` to proxies instead of cloned sender in src/api/host_tools.rs (FR-014, FR-008)

### Tests for FR-014

- [x] T045 [P] Unit test: `setup_tool_handler` can be called twice — first call succeeds, second call also succeeds after channel reset, verify new requests flow through new channel in src/api/host_tools.rs #[cfg(test)] (FR-014)
- [x] T046 [P] Unit test: in-flight invocation on old channel fails with "channel closed" after reset, new invocations use new channel in src/api/host_tools.rs #[cfg(test)] (FR-014)
- [x] T047 Unit test: re-registration after channel reset — register tool, reset channel, verify tool is still registered and proxies use new sender in src/api/host_tools.rs #[cfg(test)] (FR-014, FR-008)

**Checkpoint**: Channel re-establishment complete — mobile app can recover from backgrounding.

---

## Phase 8: FRB Compatibility (Feature-Gated)

**Purpose**: FRB StreamSink wrapper for Dart/Flutter consumption.

- [x] T048 Implement `setup_tool_handler_stream(handle, sink)` — create mpsc, call `setup_tool_handler`, spawn forwarding task from receiver to StreamSink, behind `#[cfg(feature = "frb")]` in src/api/host_tools.rs (FR-005, SC-007)
- [x] T049 [P] Add `#[cfg(feature = "frb")]` re-export for `setup_tool_handler_stream` in src/api/mod.rs (SC-007)
- [x] T050 Unit test: FRB compile gate — verify `setup_tool_handler_stream` is absent when `frb` feature is not enabled (compile-time check) in src/api/host_tools.rs #[cfg(test)] (SC-007)

**Checkpoint**: FRB wrappers ready — Dart/Flutter can consume the API via StreamSink.

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, cleanup, and final validation

- [x] T051 [P] Verify all public API functions (`register_tool`, `unregister_tool`, `setup_tool_handler`, `submit_tool_response`) are re-exported or accessible from src/api/mod.rs
- [x] T052 [P] Run `cargo clippy --all-targets -- -D warnings` and fix any warnings
- [x] T053 [P] Run `cargo fmt --all -- --check` and fix any formatting issues
- [x] T054 Run full test suite `cargo test` and verify zero failures
- [x] T055 Validate quickstart.md code examples compile (spot-check key snippets)

**Checkpoint**: Feature complete — all FRs covered, all tests pass, code is clean.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies — can start immediately
- **Phase 2 (Foundational)**: Depends on Phase 1 — BLOCKS all user stories
- **Phase 3 (US1 MVP)**: Depends on Phase 2 — core round-trip
- **Phase 4 (US2 Dynamic)**: Depends on Phase 2 — can run parallel with Phase 3
- **Phase 5 (US3 Streaming)**: Depends on Phase 3 (needs working round-trip to verify events)
- **Phase 6 (Edge Cases)**: Depends on Phase 3 (needs working execute for timeout/cancel tests)
- **Phase 7 (FR-014 Channel)**: Depends on Phase 3 (needs working setup_tool_handler to refactor)
- **Phase 8 (FRB)**: Depends on Phase 3 (needs working setup_tool_handler)
- **Phase 9 (Polish)**: Depends on all previous phases

### User Story Dependencies

- **US1 (P1)**: Depends on Foundational (Phase 2) only — no other story dependencies
- **US2 (P2)**: Depends on Foundational (Phase 2) only — can run parallel with US1
- **US3 (P2)**: Depends on US1 (Phase 3) — needs working invoke to verify events

### Within Each Phase

- Tasks marked [P] can run in parallel
- Tasks without [P] must be sequential within their phase
- Tests should be written to fail first, then pass after implementation

### Parallel Opportunities

```
Phase 1 (Setup):     T001 ─┐
                     T002 ─┤ sequential (T002 depends on T001 types)
                     T003 ─┘

Phase 2 (Foundation): T004 → T005 → T006
                      T007 ─┐
                      T008 ─┤ parallel (different methods, no deps)
                      T010 ─┘
                      T009 (depends on T004, T008)
                      T011, T012 parallel (different file from T004-T010)
                      T013 (depends on T002)
                      T014-T019 tests: T014, T015, T016, T017, T018, T019 — T14-T18 parallel, T19 after

Phase 3 (US1):       T020 → T021 → T022 → T023 → T024 (sequential — each builds on prior)
                      T025, T026, T027 parallel tests
                      T028, T029 sequential integration tests

Phase 4 (US2):       T030, T031 parallel unit tests
                      T032 integration test (after T030-T031)

Phase 5 (US3):       T033 single integration test

Phase 6 (Edge):      T034, T035, T036, T037 parallel
                      T038 (depends on registry)
                      T039 integration test

Phase 7 (FR-014):    T040 → T041 → T042 → T043 → T044 (sequential refactor chain)
                      T045, T046 parallel tests
                      T047 after T045-T046

Phase 8 (FRB):       T048 → T049 parallel with T050

Phase 9 (Polish):    T051, T052, T053 parallel → T054 → T055
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001–T003)
2. Complete Phase 2: Foundational (T004–T019)
3. Complete Phase 3: User Story 1 (T020–T029)
4. **STOP and VALIDATE**: Run `cargo test` — US1 round-trip works end-to-end
5. Deploy/demo if ready

### Incremental Delivery

1. Phase 1 + 2 → Foundation ready
2. Phase 3 (US1) → MVP: single tool registration + invocation works ✅
3. Phase 4 (US2) → Dynamic lifecycle: register/unregister anytime ✅
4. Phase 5 (US3) → Streaming parity: indistinguishable from built-in ✅
5. Phase 6 (Edge) → Robust: timeout, cancel, rebuild, malformed ✅
6. Phase 7 (FR-014) → Mobile resilience: channel re-establishment ✅
7. Phase 8 (FRB) → Flutter-ready: Dart can consume the API ✅
8. Phase 9 (Polish) → Ship: clean, tested, documented ✅

### FR Coverage Matrix

| FR | Tasks |
|----|-------|
| FR-001 | T001, T006, T016, T022 |
| FR-002 | T007, T016, T023, T030 |
| FR-003 | T008, T011, T017, T022 |
| FR-004 | T001, T009, T020, T021, T025 |
| FR-005 | T004, T010, T020, T048 |
| FR-006 | T009, T026 |
| FR-007 | T033 |
| FR-008 | T008, T011, T013, T024, T038, T044 |
| FR-009 | T006, T012, T015, T022 |
| FR-010 | T005, T006, T014 |
| FR-011 | T006, T007, T019 |
| FR-012 | T009, T034 |
| FR-013 | T013, T022, T027, T028 |
| FR-014 | T040, T041, T042, T043, T044, T045, T046, T047 |

### SC Coverage Matrix

| SC | Tasks |
|----|-------|
| SC-001 | T029 |
| SC-002 | T032 |
| SC-003 | T033 |
| SC-004 | T039 |
| SC-005 | T026 |
| SC-006 | T015 |
| SC-007 | T048, T049, T050 |
