# Implementation Plan: Mobile Port (Android + iOS) with Optional Gateway & Streaming Interface

**Branch**: `001-android-port-streaming` | **Date**: 2026-03-19 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/001-android-port-streaming/spec.md`

## Summary

Refactor ZeroClaw's gateway module behind an optional Cargo feature flag, expose a Rust-native streaming conversation API and runtime configuration API suitable for flutter_rust_bridge (FRB) consumption, and ensure the core library compiles for Android (aarch64-linux-android, armv7-linux-androideabi) and iOS (aarch64-apple-ios) targets. The streaming API uses Rust async Streams that FRB translates to Dart Streams. Runtime config changes apply live without restart. Secrets are injected at runtime, not read from config files.

## Technical Context

**Language/Version**: Rust (stable, edition 2021)
**Primary Dependencies**: tokio (async runtime), serde/toml (config), flutter_rust_bridge (FFI to Dart/Flutter); axum/tower/rust-embed (gateway-only, conditional)
**Storage**: SQLite (memory backend, session persistence), TOML files (config)
**Testing**: cargo test / cargo nextest; integration tests in `tests/`
**Target Platform**: Android (aarch64-linux-android, armv7-linux-androideabi), iOS (aarch64-apple-ios), Linux/macOS/Windows (existing)
**Project Type**: Library (core) + CLI (existing, gateway-optional)
**Performance Goals**: First streaming chunk within 500ms of model response start; config reload <1s
**Constraints**: Binary size with gateway disabled ≥20% smaller than full build; no HTTP deps linked when gateway disabled
**Scale/Scope**: Single-process agent runtime embedded in mobile app; one concurrent conversation per instance

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution is not yet initialized for this project (template only). No gate violations to check. Proceeding with CLAUDE.md project rules:

- [x] **One concern per PR** — this feature is cohesive (gateway optional + mobile API + live config)
- [x] **Minimal patch** — no speculative abstractions; every new type has a concrete use case
- [x] **No heavy deps for minor convenience** — flutter_rust_bridge is required for FRB, not convenience
- [x] **No security weakening** — secrets injected at runtime (FR-015), not stored in plaintext config
- [x] **Risk tier**: High (touches `src/gateway/**`, `src/tools/**` boundaries, `Cargo.toml` feature flags)

## Project Structure

### Documentation (this feature)

```text
specs/001-android-port-streaming/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output (library API contracts)
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
src/
├── api/                 # NEW: Public library API surface (conversation, config, observer)
│   ├── mod.rs           # Re-exports
│   ├── types.rs         # Shared types: StreamEvent, ApiError, ConfigPatch, ObserverEventDto
│   ├── conversation.rs  # ConversationStream, send_message(), cancel_message()
│   ├── config.rs        # RuntimeConfigManager, get/update/reload/subscribe
│   ├── lifecycle.rs     # AgentHandle, init(), shutdown()
│   └── observer.rs      # ObserverCallbackRegistry, register/unregister observer callbacks
├── agent/               # EXISTING: Orchestration loop (add streaming event emission)
│   ├── agent.rs
│   └── loop_.rs
├── config/              # EXISTING: Schema + loading (add live reload, change notification)
│   ├── schema.rs
│   └── workspace.rs
├── gateway/             # EXISTING: HTTP server (gate behind #[cfg(feature = "gateway")])
│   ├── mod.rs
│   ├── api.rs
│   ├── ws.rs
│   ├── sse.rs
│   └── ...
├── observability/       # EXISTING: Observer trait (no changes needed)
│   └── traits.rs
├── providers/           # EXISTING: Provider trait with streaming (no changes needed)
│   └── traits.rs
├── lib.rs               # MODIFY: Conditional gateway export, add pub mod api
└── main.rs              # MODIFY: Conditional gateway CLI commands

Cargo.toml               # MODIFY: Add gateway feature flag, conditional deps

tests/
├── api_streaming_test.rs    # NEW: ConversationStream contract tests
├── api_config_test.rs       # NEW: RuntimeConfig API tests
└── gateway_feature_test.rs  # NEW: Verify gateway feature flag compilation
```

**Structure Decision**: Single project (Rust workspace root). New `src/api/` module provides the public library interface consumed by FRB. Existing modules are modified in-place with `#[cfg(feature = "gateway")]` guards. No new crates or workspace members.

## Complexity Tracking

No violations to track — no constitution gates active.

---

## Phase 0: Research (COMPLETE)

All unknowns from Technical Context resolved. See [research.md](research.md).

| ID    | Topic              | Decision                                                                |
| ----- | ------------------ | ----------------------------------------------------------------------- |
| R-001 | Gateway coupling   | 6 surgical `#[cfg]` guards at identified coupling points                |
| R-002 | Streaming agent    | New `turn_streaming()` + mpsc → Stream, alongside existing `turn()`     |
| R-003 | FRB compatibility  | `StreamSink<StreamEvent>` pattern for Rust→Dart streaming               |
| R-004 | Live config reload | `watch` channel + subsystem reinit; in-flight completes with old config |
| R-005 | Feature flags      | `gateway` feature with optional deps, added to `default` features       |
| R-006 | Cross-compilation  | No platform-specific code; standard Rust NDK toolchain                  |

## Phase 1: Design (COMPLETE)

All design artifacts generated:

| Artifact     | Path                                                 | Content                                                                                                 |
| ------------ | ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| Data Model   | [data-model.md](data-model.md)                       | StreamEvent, AgentHandle, RuntimeConfigManager, ConfigPatch, ObserverCallbackRegistry, ObserverEventDto |
| API Contract | [contracts/library-api.md](contracts/library-api.md) | 4 API modules: conversation, config, observer, lifecycle                                                |
| Quickstart   | [quickstart.md](quickstart.md)                       | Flutter integration guide with FRB                                                                      |

Agent context updated via `update-agent-context.ps1 -AgentType copilot`.

## Constitution Re-Check (Post-Design)

Constitution is template-only (no project-specific gates). Re-checking CLAUDE.md rules against the design:

- [x] **One concern per PR** — design is internally consistent; all new types serve the mobile port
- [x] **Minimal patch** — no speculative abstractions. Every type (StreamEvent, ConfigPatch, AgentHandle, ObserverCallbackRegistry) maps directly to a spec FR
- [x] **No heavy deps** — flutter_rust_bridge is the only new dependency, required by the spec
- [x] **No security weakening** — secrets injected at runtime only (FR-015); no new attack surface without gateway
- [x] **Risk tier acknowledged** — High risk; full CI validation required before merge
- [x] **No unrelated modules modified** — design changes are scoped to `src/api/` (new), `src/agent/` (streaming addition), `src/config/` (watch channel), `src/lib.rs` + `src/main.rs` (feature gates), `Cargo.toml` (feature flags)

**Gate status: PASS** — proceed to Phase 2 task breakdown.
