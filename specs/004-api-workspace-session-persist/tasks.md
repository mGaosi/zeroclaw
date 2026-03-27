# Tasks: API Workspace Directory & Session Persistence

**Input**: Design documents from `/specs/004-api-workspace-session-persist/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Tests for public API surface are MANDATORY per constitution Principle IV. Every phase that introduces public API functions MUST include a corresponding test task.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup

Skipped — this feature extends an existing Rust project. No new project initialization, dependencies, or toolchain configuration required.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared types and struct fields that multiple user stories depend on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T001 [P] Add workspace_dir field to ConfigPatch (with Default) and validate_session_key() function in src/api/types.rs
- [x] T002 [P] Add session_backend (Arc<RwLock<Option<Arc<dyn SessionBackend>>>>) and current_session_key (Arc<Mutex<Option<String>>>) fields to AgentHandle in src/api/lifecycle.rs

**Checkpoint**: Foundational types ready — user story implementation can begin

---

## Phase 3: User Story 1 — Configure Workspace Directory at Initialization (Priority: P1) 🎯 MVP

**Goal**: Allow API callers to specify a writable workspace directory during init(), essential for mobile platforms where default home directory resolution fails.

**Independent Test**: Initialize an agent with a custom workspace path, verify all file-based operations use that path as root. Verify validation rejects unwritable or invalid paths with clear errors.

**Requirements**: FR-001, FR-002, FR-003, FR-008, SC-001, SC-006

### Tests for User Story 1

- [x] T003 [P] [US1] Add unit tests for ConfigPatch.workspace_dir defaults and validate_session_key() validation rules in src/api/types.rs (#[cfg(test)] module)
- [x] T003b [US1] Add integration test for init() with workspace_dir: valid path, non-existent path (auto-created), unwritable path (returns ValidationError), and None fallback — in tests/integration/api_session_test.rs

### Implementation for User Story 1

- [x] T004 [US1] Implement workspace_dir validation (create_dir_all + write-test) in init() returning ApiError::ValidationError on failure in src/api/lifecycle.rs
- [x] T005 [US1] Initialize session backend (SqliteSessionBackend or SessionStore based on config) during init() when workspace and session_persistence are available in src/api/lifecycle.rs

**Checkpoint**: Agent can be initialized with a custom workspace directory. Session backend is ready for use by subsequent stories.

---

## Phase 4: User Story 2 — Persist Complete Conversations in API Mode (Priority: P1)

**Goal**: Automatically save complete conversations (all message roles, tool calls, tool results) to disk so they survive app restarts, process crashes, and memory pressure.

**Independent Test**: Send messages through the API, verify the full conversation (including tool interactions) is written to disk. Restart the process, send a message with the same session key, verify prior history is auto-loaded into agent context.

**Requirements**: FR-004, FR-005, FR-006, FR-009, FR-012, FR-013, FR-014, SC-002, SC-003

### Tests for User Story 2

- [x] T006 [US2] Create integration test file with persistence round-trip tests (send → restart → resume), session switching tests, and SC-003 performance assertion (append < 50ms per turn) in tests/integration/api_session_test.rs (also register module in tests/integration/mod.rs)

### Implementation for User Story 2

- [x] T007 [US2] Add session_key: Option<String> parameter to send_message() and send_message_stream() in src/api/conversation.rs (defaults to "api_default" when None)
- [x] T008 [US2] Add conversation_messages_to_chat_messages() converter function mapping ConversationMessage variants to ChatMessage records in src/api/conversation.rs
- [x] T009 [US2] Implement session key tracking via current_session_key — on key change: persist current history, clear agent history, auto-load new session via seed_history() in src/api/conversation.rs
- [x] T010 [US2] Implement post-turn message persistence (append new messages to backend) with non-blocking error handling (StreamEvent::Error + warn log) in src/api/conversation.rs
- [x] T011 [P] [US2] Add #[cfg(unix)] set_permissions(0o600) on JSONL session files at creation time in src/channels/session_store.rs
- [x] T012 [P] [US2] Add #[cfg(unix)] set_permissions(0o600) on SQLite session DB at creation time in src/channels/session_sqlite.rs

**Checkpoint**: Conversations are automatically persisted to disk and resumable after restart. File permissions protect session data on Unix platforms.

---

## Phase 5: User Story 3 — Manage Conversation Sessions (Priority: P2)

**Goal**: Provide list, load, and delete operations so multi-conversation applications can manage session threads.

**Independent Test**: Create multiple sessions via send_message, list all sessions (verify metadata), load one by key (verify full history), delete another (verify removal from listings).

**Requirements**: FR-007, SC-004, SC-005

### Tests for User Story 3

- [x] T013 [US3] Add integration tests for list_sessions(), load_session_history(), and delete_session() in tests/integration/api_session_test.rs — include SC-004 smoke test: create 100 sessions and verify list completes in < 500ms

### Implementation for User Story 3

- [x] T014 [P] [US3] Add SessionInfo struct (key, message_count, created_at, last_activity) with Serialize/Deserialize in src/api/types.rs
- [x] T015 [US3] Implement list_sessions() returning Vec<SessionInfo> sorted by last_activity descending in src/api/conversation.rs
- [x] T016 [US3] Implement load_session_history() returning Vec<ChatMessage> for a given session key in src/api/conversation.rs
- [x] T017 [US3] Implement delete_session() removing all persisted data for a session key in src/api/conversation.rs
- [x] T018 [US3] Export SessionInfo, list_sessions, load_session_history, delete_session from src/api/mod.rs

**Checkpoint**: Full session CRUD available (create implicit via send_message, read/list/delete explicit). Multi-conversation apps can manage threads.

---

## Phase 6: User Story 4 — Configure Workspace Directory at Runtime (Priority: P3)

**Goal**: Support changing workspace directory after initialization for multi-profile or multi-tenant scenarios.

**Independent Test**: Initialize agent with workspace A, update workspace to B via config API, verify session operations now target workspace B. Verify invalid path is rejected and agent continues using previous workspace.

**Requirements**: FR-010, FR-011

### Tests for User Story 4

- [x] T019 [US4] Add integration tests for runtime workspace change (valid switch + invalid rejection) in tests/integration/api_session_test.rs

### Implementation for User Story 4

- [x] T020 [US4] Handle workspace_dir field in update_config() rebuild flow with validation in src/api/config.rs
- [x] T021 [US4] Re-initialize session backend on workspace change in apply_config_changes_if_needed() in src/api/conversation.rs

**Checkpoint**: Workspace directory can be changed at runtime with automatic session backend re-initialization. Invalid paths are rejected gracefully.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Documentation and validation that span multiple user stories

- [x] T022 [P] Create concurrent access warning documentation (FR-015) in docs/reference/api/session-persistence.md
- [x] T023 Run quickstart.md validation scenarios to verify all 4 usage examples work end-to-end

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: Skipped — existing project
- **Foundational (Phase 2)**: No dependencies — can start immediately. BLOCKS all user stories.
- **User Story 1 (Phase 3)**: Depends on Phase 2 completion
- **User Story 2 (Phase 4)**: Depends on Phase 2 + US1 (requires workspace dir + session backend initialization)
- **User Story 3 (Phase 5)**: Depends on Phase 2 + US2 (requires sessions to exist for management)
- **User Story 4 (Phase 6)**: Depends on Phase 2 + US1 (extends workspace config). Can run in parallel with US2/US3 if needed.
- **Polish (Phase 7)**: Depends on all user stories being complete

### User Story Dependencies

- **US1 (P1)**: Depends on Foundational only — MVP deliverable
- **US2 (P1)**: Depends on US1 (session backend initialized during init)
- **US3 (P2)**: Depends on US2 (sessions must be persistable before they can be managed)
- **US4 (P3)**: Depends on US1 (extends workspace config). Independent of US2/US3.

### Within Each User Story

- Tests written first (Constitution Principle IV)
- Types/models before logic
- Core logic before API surface
- Integration verified before checkpoint

### Parallel Opportunities

**Phase 2** (Foundational):
- T001 ∥ T002 — different files (types.rs vs lifecycle.rs)

**Phase 3** (US1):
- T003 can run parallel with T004 (test file vs implementation file)

**Phase 4** (US2):
- T011 ∥ T012 — different files (session_store.rs vs session_sqlite.rs)
- T011 ∥ T012 can run parallel with T007–T010 (channels/ vs api/)

**Phase 5** (US3):
- T014 can run parallel with T013 (types.rs vs test file)

**Phase 6** (US4):
- T019 can run parallel with T020 (test file vs config.rs)

**Cross-story parallelism**:
- US4 can start in parallel with US2 if US1 is complete (different dependency chains)

---

## Parallel Example: User Story 2

```bash
# These can run at the same time (different files):
Task T011: "#[cfg(unix)] 0600 permissions on JSONL in src/channels/session_store.rs"
Task T012: "#[cfg(unix)] 0600 permissions on SQLite in src/channels/session_sqlite.rs"

# These run sequentially (same file: src/api/conversation.rs):
Task T007 → T008 → T009 → T010
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 2: Foundational (T001–T002)
2. Complete Phase 3: User Story 1 (T003–T005)
3. **STOP and VALIDATE**: Agent initializes with custom workspace, validation works
4. This is the minimum useful increment — workspace dir config works

### Incremental Delivery

1. Phase 2 → Foundation ready
2. Phase 3: US1 → Custom workspace initialization works (MVP!)
3. Phase 4: US2 → Conversations survive restarts
4. Phase 5: US3 → Multi-conversation management available
5. Phase 6: US4 → Runtime workspace switching for power users
6. Phase 7: Polish → Documentation and validation complete
7. Each story adds value without breaking previous stories

### Sequential Developer Strategy

One developer completing all stories:

1. T001 + T002 (Foundational) → immediate
2. T003–T005 (US1) → workspace config works
3. T006–T012 (US2) → persistence works
4. T013–T018 (US3) → session management works
5. T019–T021 (US4) → runtime workspace change works
6. T022–T023 (Polish) → docs and validation

---

## Requirement Traceability

| Requirement | Tasks | Story |
|-------------|-------|-------|
| FR-001 (init workspace_dir) | T001, T004 | US1 |
| FR-002 (validate writable) | T003b, T004 | US1 |
| FR-003 (propagate to subsystems) | T004, T005 | US1 |
| FR-004 (session persistence) | T008, T009, T010 | US2 |
| FR-005 (reuse SessionBackend) | T002, T005 | US1 |
| FR-006 (session key) | T007 | US2 |
| FR-007 (list/load/delete) | T015, T016, T017 | US3 |
| FR-008 (persistence default on) | T005 | US1 |
| FR-009 (append-only safety) | T010 | US2 |
| FR-010 (runtime workspace_dir) | T020 | US4 |
| FR-011 (rebuild on change) | T020, T021 | US4 |
| FR-012 (auto-load history) | T009 | US2 |
| FR-013 (0600 permissions) | T011, T012 | US2 |
| FR-014 (non-blocking errors) | T010 | US2 |
| FR-015 (concurrent access docs) | T022 | Polish |
| SC-003 (<50ms per turn) | T006 | US2 |
| SC-004 (1000 sessions) | T013 | US3 |
| SC-006 (catch unwritable paths) | T003, T003b | US1 |

---

## Notes

- [P] tasks = different files, no dependencies on incomplete tasks
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable at its checkpoint
- All test tasks precede implementation per Constitution Principle IV
- Commit after each task or logical group
- File paths reference existing project structure (single-project Rust layout)
