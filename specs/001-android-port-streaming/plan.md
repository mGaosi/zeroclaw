# Implementation Plan: Mobile Port (Android + iOS) with Optional Gateway & Streaming Interface

**Branch**: `001-android-port-streaming` | **Date**: 2026-03-24 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/001-android-port-streaming/spec.md`

## Summary

Port ZeroClaw to Android and iOS as an embeddable Rust library, gate the gateway module behind an optional Cargo feature flag, expose a streaming conversation API via `src/api/` for FRB code generation, support runtime configuration changes with watch-based subsystem notification, and provide an observer callback interface for host-side observability — all without requiring the HTTP/WebSocket gateway.

## Technical Context

**Language/Version**: Rust (stable, edition 2021)
**Primary Dependencies**: tokio (async runtime), serde/toml (config), reqwest+rustls (HTTP), rusqlite+bundled (storage), flutter_rust_bridge 2.11 (FFI, optional `frb` feature)
**Storage**: SQLite via rusqlite (bundled for Android/iOS cross-compilation)
**Testing**: `cargo test` / `cargo nextest run`, inline `#[cfg(test)]` modules, `tests/` (component, integration, system)
**Target Platform**: aarch64-linux-android, armv7-linux-androideabi, aarch64-apple-ios, Linux, macOS, Windows
**Project Type**: Library (core) + CLI; `src/api/` provides the public library interface consumed by FRB
**Performance Goals**: First streaming chunk within 500ms of model response start (SC-007); binary size ≥20% smaller with gateway disabled (SC-005)
**Constraints**: Must run on ARM mobile devices with limited RAM; no blocking the tokio runtime; no platform-specific code in core
**Scale/Scope**: 5 user stories, 15 functional requirements, 8 success criteria; touches `src/api/`, `src/agent/`, `src/config/`, `src/lib.rs`, `src/main.rs`, `Cargo.toml`

## Constitution Check (Pre-Research)

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Status | Evidence |
|---|-----------|--------|----------|
| I | Trait-Driven Modularity | ✅ PASS | Observer callback uses existing `Observer` trait; provider streaming uses existing `Provider` trait methods; no core orchestration modifications — new behavior added via `src/api/` module and `turn_streaming()` method |
| II | Read Before Write | ✅ PASS | Explore subagents analyzed gateway coupling (17 points), `Agent` struct fields (15+), `turn_streaming()` (L808-1045), `Provider` trait (streaming methods), `Observer` trait, `Config` struct, `src/api/` module (6 files) before planning changes |
| III | Minimal Patch | ✅ PASS | Uses surgical `#[cfg(feature = "gateway")]` guards at specific coupling points; no speculative abstractions; `GatewayConfig` stays unconditional in schema; reuses existing `turn_streaming()` rather than creating new streaming infrastructure |
| IV | Public API Testing (NON-NEGOTIABLE) | ✅ PLANNED | Every Phase 1 design artifact defines test coverage: `send_message` (streaming events), `update_config` (validation/rejection), `reload_config_from_file` (merge semantics), `register_observer` (event delivery), `init`/`shutdown` (lifecycle). Tasks will include mandatory test tasks per constitution. |
| V | Security by Default | ✅ PASS | Secrets injectable at runtime only (FR-015, no TOML secrets on Android); gateway removal reduces attack surface; workspace isolation unchanged; `src/tools/` changes limited to `#[cfg]` gating (no behavior change) |
| VI | Task Clarity | ✅ PLANNED | Tasks will include unique IDs, exact file paths, user story references, parallelism markers, and dependency declarations per constitution |
| VII | Performance Discipline | ✅ PASS | Gateway feature flag reduces binary size (SC-005); `watch` channel for config avoids polling; `mpsc` for streaming avoids allocation-heavy patterns; no new heavy dependencies |

**Gate result**: PASS — no violations. Proceed to Phase 0.

## Project Structure

### Documentation (this feature)

```text
specs/001-android-port-streaming/
├── plan.md              # This file
├── research.md          # Phase 0 output (10 research decisions)
├── data-model.md        # Phase 1 output (5 entities)
├── quickstart.md        # Phase 1 output (Flutter SDK guide)
├── contracts/
│   └── library-api.md   # Phase 1 output (11 API functions)
└── tasks.md             # Phase 2 output (created by /speckit.tasks)
```

### Source Code (affected modules)

```text
src/
├── api/                    # PUBLIC LIBRARY INTERFACE (FRB-consumed)
│   ├── mod.rs              # Re-exports, feature-gated FRB wrappers
│   ├── types.rs            # StreamEvent, ConfigPatch, ObserverEventDto, ApiError
│   ├── conversation.rs     # send_message, cancel_message
│   ├── config.rs           # RuntimeConfigManager, get/update/reload config
│   ├── lifecycle.rs        # AgentHandle, init, shutdown
│   └── observer.rs         # ObserverCallbackRegistry, register/unregister
├── agent/
│   └── agent.rs            # turn_streaming() (L808-1045, existing)
├── config/
│   └── schema.rs           # Config struct, GatewayConfig (stays unconditional)
├── gateway/                # FEATURE-GATED (entire module)
├── lib.rs                  # #[cfg(feature = "gateway")] mod gateway (existing)
├── main.rs                 # #[cfg(feature = "gateway")] on CLI variants (needs fix)
├── tools/
│   ├── mod.rs              # #[cfg(feature = "gateway")] on node_tool (existing)
│   └── node_tool.rs        # Gateway-dependent (gated at module level)
├── daemon/
│   └── mod.rs              # #[cfg(feature = "gateway")] on gateway spawn (existing)
├── providers/
│   └── traits.rs           # Provider trait with streaming methods (existing)
└── observability/
    └── traits.rs           # Observer trait (existing)

tests/
├── component/              # Component-level tests
├── integration/            # Integration tests
└── system/                 # System (E2E) tests

Cargo.toml                  # Feature flag definitions, optional deps
```

## Complexity Tracking

> No Constitution Check violations requiring justification.

---

## Phase 0: Research Output

All unknowns resolved. See [research.md](research.md) for full details.

| ID | Topic | Decision | Risk |
|----|-------|----------|------|
| R-001 | Gateway coupling | 17 coupling points found; 14 already `#[cfg]`-gated; 3 critical fixes needed (`src/main.rs` L249, L976; `src/tools/node_tool.rs` module-level gate sufficient) | Low |
| R-002 | Agent streaming | `turn_streaming()` already exists at `agent.rs` L808-1045; uses `mpsc::Sender<StreamEvent>` + `CancellationToken`; calls `chat()` not `stream_chat_with_history()` yet | Low |
| R-003 | FRB compatibility | `StreamSink<StreamEvent>` pattern; concrete types only in `src/api/`; no `dyn Trait`/`impl Trait` in public API | Low |
| R-004 | Config live reload | `RuntimeConfigManager` with `watch` channel for subsystem notification; `update_config()` validates → merges → notifies; in-flight requests unaffected | Medium |
| R-005 | Feature flags | `gateway` feature with 6 optional deps; already in `default` features; `default-features = false` excludes gateway | Low |
| R-006 | Cross-compilation | No platform-specific code needed; `rusqlite` bundled; `reqwest` + `rustls`; tokio on background thread (FRB isolate) | Low |
| R-007 | Concurrent conversations | Serial within `AgentHandle` via `Arc<Mutex<Agent>>`; parallel via separate handles | Low |
| R-008 | Config merge semantics | File values overwrite matching fields; absent fields retain in-memory values (preserves runtime-injected secrets) | Low |
| R-009 | Non-streaming fallback | Single `Chunk` + `Done` for providers with `supports_streaming() == false` | Low |
| R-010 | History preservation | `ChatMessage` format survives provider swap; history vector stays in `Agent` | Low |

---

## Phase 1: Design Output

### Data Model — [data-model.md](data-model.md)

5 entities defined:

| Entity | Type | Status |
|--------|------|--------|
| `StreamEvent` | New enum (`Chunk`, `ToolCall`, `ToolResult`, `Done`, `Error`) | Output-only; mirrors WebSocket protocol |
| `AgentHandle` | New struct | Owns `Arc<Mutex<Agent>>`, `RuntimeConfigManager`, `ObserverCallbackRegistry`, `CancellationToken` |
| `RuntimeConfigManager` | New struct | `Arc<Mutex<Config>>` + `watch::Sender<Config>` + optional file path |
| `ConfigPatch` | New struct | Partial config update (all `Option` fields) |
| `ObserverCallbackRegistry` | New struct | `HashMap<u64, mpsc::Sender<ObserverEventDto>>` + atomic ID counter; implements `Observer` trait |

Modified entities:
- `Config` (existing): No structural changes; `GatewayConfig` stays unconditional
- `Agent` (existing): `turn_streaming()` already implemented at L808-1045; no modifications needed

### API Contracts — [contracts/library-api.md](contracts/library-api.md)

11 API functions across 4 modules in `src/api/`:

| Module | Function | Signature Summary | FR Coverage |
|--------|----------|-------------------|-------------|
| `lifecycle.rs` | `init` | `(config_path: Option<String>, overrides: Option<ConfigPatch>) → Result<AgentHandle, ApiError>` | FR-002, FR-008 |
| `lifecycle.rs` | `shutdown` | `(handle: AgentHandle) → Result<(), ApiError>` | — |
| `conversation.rs` | `send_message` | `(handle, message, sink: StreamSink<StreamEvent>) → Result<(), ApiError>` | FR-003, FR-004, FR-012 |
| `conversation.rs` | `cancel_message` | `(handle) → Result<(), ApiError>` | FR-012 |
| `config.rs` | `get_config` | `(handle) → Result<String, ApiError>` | FR-005 |
| `config.rs` | `update_config` | `(handle, patch: ConfigPatch) → Result<(), ApiError>` | FR-005, FR-006, FR-007, FR-013, FR-015 |
| `config.rs` | `reload_config_from_file` | `(handle) → Result<(), ApiError>` | FR-009 |
| `observer.rs` | `register_observer` | `(handle, sink: StreamSink<ObserverEventDto>) → Result<u64, ApiError>` | FR-014 |
| `observer.rs` | `unregister_observer` | `(handle, observer_id: u64) → Result<(), ApiError>` | FR-014 |

FRB-gated wrappers (`#[cfg(feature = "frb")]`):
- `send_message_stream` — FRB `StreamSink` variant
- `register_observer_stream` — FRB `StreamSink` variant

### Quickstart — [quickstart.md](quickstart.md)

Flutter SDK integration guide covering: prerequisites, dependency setup, FRB codegen, init, streaming messages, config updates, observer registration, shutdown, Android/iOS builds, and common issues.

---

## Constitution Re-Check (Post-Design)

*GATE: Must pass before Phase 2 task generation.*

| # | Principle | Status | Evidence |
|---|-----------|--------|----------|
| I | Trait-Driven Modularity | ✅ PASS | `ObserverCallbackRegistry` implements existing `Observer` trait. No new traits introduced. Provider streaming uses existing `Provider::chat()`. All extension via trait implementation and factory registration. |
| II | Read Before Write | ✅ PASS | Explore subagents read `agent.rs` (1045+ lines), `traits.rs` (Provider, Observer), `schema.rs` (Config), `mod.rs` (api), `lib.rs`, `main.rs`, `tools/mod.rs`, `node_tool.rs`, `daemon/mod.rs` before any design decisions. Research.md documents 17 gateway coupling points with exact line numbers. |
| III | Minimal Patch | ✅ PASS | R-001 requires only 3 fixes (2 `#[cfg]` guards in main.rs). R-002 confirms `turn_streaming()` already exists — no new agent code. `src/api/` module already exists with 6 files. `GatewayConfig` kept unconditional. No speculative abstractions. |
| IV | Public API Testing (NON-NEGOTIABLE) | ✅ PLANNED | Design defines 11 API functions. Task generation (Phase 2) MUST include test tasks for each: `init`/`shutdown` lifecycle, `send_message` streaming events (happy path + cancellation + error), `update_config` (valid + invalid + in-flight), `reload_config_from_file` (merge + missing file + invalid), `register_observer`/`unregister_observer` (event delivery). Tests MUST cover acceptance scenarios from spec. |
| V | Security by Default | ✅ PASS | FR-015 enforced: secrets injectable at runtime only, never in TOML. Gateway removal reduces attack surface on mobile. No changes to security policy module. `src/tools/` changes limited to existing `#[cfg]` gating. |
| VI | Task Clarity | ✅ PLANNED | Phase 2 (`/speckit.tasks`) will generate tasks with unique IDs, exact file paths, user story references, parallelism markers, and dependency declarations per constitution. |
| VII | Performance Discipline | ✅ PASS | `watch` channel avoids polling. `mpsc` for streaming is zero-copy for `String` deltas. Binary size reduction via gateway exclusion (SC-005). No new heavy dependencies. `AtomicU64` for observer IDs avoids mutex contention. |

**Gate result**: PASS — all 7 principles satisfied. Proceed to Phase 2 task generation (`/speckit.tasks`).
