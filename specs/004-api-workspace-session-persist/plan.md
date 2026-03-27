# Implementation Plan: API Workspace Directory & Session Persistence

**Branch**: `004-api-workspace-session-persist` | **Date**: 2026-03-27 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/004-api-workspace-session-persist/spec.md`

## Summary

Add workspace directory configuration to the ZeroClaw embedded API (`init()` + `ConfigPatch`) so mobile/embedded callers can specify a writable storage path. Integrate the existing `SessionBackend` infrastructure (SQLite/JSONL) into the API layer to persist complete conversations to disk, with session key support for multi-conversation apps and management functions (list/load/delete).

**Technical approach**: Extend `ConfigPatch` with `workspace_dir` field (R1). Add `session_backend: Arc<RwLock<Option<Arc<dyn SessionBackend>>>>` and `current_session_key: Arc<Mutex<Option<String>>>` to `AgentHandle` (R2). Add `session_key` parameter to `send_message()` (R3). Auto-load persisted history on first message via `seed_history()` (R4). Validate workspace at init (R5). Non-blocking persistence with error events (R6). Enforce 0600 file permissions on Unix session files (R7).

## Technical Context

**Language/Version**: Rust (stable, edition 2021)
**Primary Dependencies**: tokio 1.50 (async runtime), rusqlite 0.37 (SQLite, bundled), serde/toml (config), flutter_rust_bridge 2.11 (FRB, optional feature), chrono 0.4 (timestamps), thiserror 2.0 (error derives)
**Storage**: SQLite (WAL mode, FTS5) and JSONL for sessions; existing `SessionBackend` trait with both implementations
**Testing**: `cargo test` (inline `#[cfg(test)]` modules + `tests/integration/` integration tests), `cargo nextest run`
**Target Platform**: Linux, macOS, Android (via FRB/Flutter), iOS (via FRB/Flutter), ARM SoCs
**Project Type**: Library (embedded agent runtime consumed via FFI/FRB)
**Performance Goals**: Session append <1ms; session load <50ms for 1000 messages; no perceptible latency added to conversation round-trips (SC-003: <50ms per turn)
**Constraints**: <5MB RAM overhead for session backend; no new dependencies; non-blocking persistence
**Scale/Scope**: 1,000+ sessions per workspace (SC-004), 10,000+ messages per session

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Pre-Research Gate (Phase 0)

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Trait-Driven Modularity | PASS | Reuses existing `SessionBackend` trait — no new traits needed. No core orchestration changes. |
| II. Read Before Write | PASS | Full codebase exploration completed: API layer (7 files), session backends (3 files), Agent struct, Config schema. |
| III. Minimal Patch | PASS | Only adds what's needed: 1 new ConfigPatch field, 2 new AgentHandle fields, 1 new send_message param, 3 new API functions, 1 new type. No speculative features. |
| IV. Public API Testing | REQUIRES | All new public functions MUST have tests: init with workspace_dir, send_message with session_key, session management functions. Tests planned in task breakdown. |
| V. Security by Default | PASS | workspace_dir validated at init. File permissions 0600 enforced on Unix via `#[cfg(unix)]` — consistent with 6+ existing usages in the codebase (runtime_trace.rs, main.rs, update.rs, service/mod.rs). Path traversal blocked by session key validation (alphanumeric/`_`/`-` only). Classified MEDIUM risk (src/api changes, no security module changes). |
| VI. Task Clarity | REQUIRES | Tasks must include file paths, user story refs, parallel markers, and unique IDs. Enforced in tasks.md generation. |
| VII. Performance Discipline | PASS | SQLite WAL append is <1ms. No new dependencies. Session backend behind Arc (shared, not cloned). No blocking the tokio runtime. |

**Gate result**: PASS — all principles satisfied or have planned mitigation (IV, VI addressed in task generation phase).

### Post-Design Gate (Phase 1)

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Trait-Driven Modularity | PASS | Design reuses `SessionBackend` trait unchanged. `ConversationMessage → ChatMessage` conversion is a simple mapping function, not a new abstraction. |
| II. Read Before Write | PASS | data-model.md and contracts/api.md reference exact existing types and methods verified against codebase. |
| III. Minimal Patch | PASS | 5 files modified, 1 new type (`SessionInfo`), 3 new functions, 1 new conversion utility. No unused config keys. |
| IV. Public API Testing | PLAN | Tests required: ConfigPatch.workspace_dir validation (unit), init with workspace (integration), send_message with session key — happy + error + switch (integration), session management CRUD (integration), persistence round-trip (integration). |
| V. Security by Default | PASS | FR-013 (file permissions 0600 on Unix, creation-time only) and FR-015 (concurrent access warning) addressed in design. On non-Unix platforms, system relies on OS-level user-profile directory isolation per spec clarification (Session 2026-03-27). No weakening of existing security policy. |
| VI. Task Clarity | PLAN | Enforced in tasks.md generation step. |
| VII. Performance Discipline | PASS | No new allocations in hot path. SessionBackend is lazy-initialized. Persistence is post-turn (no latency impact on streaming). Permission enforcement is creation-time only per spec clarification (Session 2026-03-27). |

**Gate result**: PASS

## Project Structure

### Documentation (this feature)

```text
specs/004-api-workspace-session-persist/
├── plan.md              # This file
├── spec.md              # Feature specification
├── research.md          # Phase 0: research decisions (R1–R9)
├── data-model.md        # Phase 1: entity definitions and relationships
├── quickstart.md        # Phase 1: usage examples
├── contracts/
│   └── api.md           # Phase 1: public API contract
├── checklists/
│   └── requirements.md  # Quality checklist
└── tasks.md             # Phase 2: task breakdown (generated by /speckit.tasks)
```

### Source Code (files modified by this feature)

```text
src/api/
├── types.rs             # ConfigPatch + workspace_dir field, new SessionInfo type, validate_session_key()
├── lifecycle.rs         # AgentHandle + session_backend + current_session_key fields, init() workspace validation
├── conversation.rs      # send_message() + session_key, conversation_messages_to_chat_messages(), persistence hooks, list/load/delete session mgmt functions
├── config.rs            # update_config() workspace_dir handling in rebuild flow
└── mod.rs               # New public exports (SessionInfo, list_sessions, load_session_history, delete_session)

src/channels/
├── session_backend.rs   # (unchanged — SessionBackend trait reused as-is)
├── session_store.rs     # File permission enforcement (0600) on JSONL files via #[cfg(unix)]
└── session_sqlite.rs    # File permission enforcement (0600) on SQLite DB via #[cfg(unix)]

tests/integration/
├── api_session_test.rs  # Integration tests for session persistence (NEW)
└── mod.rs               # Updated to include api_session_test module

docs/reference/api/
└── session-persistence.md  # FR-015 concurrent access warning documentation (NEW)
```

**Structure Decision**: Single-project Rust layout. All changes are within the existing `src/api/` module and adjacent `src/channels/` session backends. One new integration test file. No new crates or modules.

## Complexity Tracking

No constitution violations requiring justification. All changes follow existing patterns in the codebase.
