# Implementation Plan: Host-Side Tool Registration for Flutter

**Branch**: `002-host-tool-registration` | **Date**: 2026-03-24 | **Spec**: `specs/002-host-tool-registration/spec.md`
**Input**: Feature specification from `/specs/002-host-tool-registration/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/plan-template.md` for the execution workflow.

## Summary

Enable Flutter/Dart host apps to register custom tools that ZeroClaw's agent can invoke during conversations. The host app provides a tool name, description, and parameter schema; ZeroClaw includes the tool in the LLM's tool catalog. When the LLM invokes a host tool, ZeroClaw sends a structured request over a shared async channel (multiplexed by request ID) and waits for the host app's response. The implementation mirrors the existing `ObserverCallbackRegistry` pattern — a `HostToolRegistry` stored in `AgentHandle` that survives agent rebuilds and bridges to Dart via FRB `StreamSink`.

## Technical Context

**Language/Version**: Rust 1.87, edition 2021
**Primary Dependencies**: tokio 1.50 (async runtime, mpsc channels), serde/serde_json (serialization), parking_lot (synchronous Mutex), async-trait, flutter_rust_bridge (FRB, behind `frb` feature flag)
**Storage**: N/A — in-memory registry only
**Testing**: `cargo test` / `cargo nextest run`, inline `#[cfg(test)]` modules + `tests/` integration tests
**Target Platform**: Android (primary via FRB), Linux/macOS/Windows (library consumers), ARM SoCs, constrained devices
**Project Type**: Library (Rust crate consumed via FFI/FRB by Flutter apps)
**Performance Goals**: Tool registration/unregistration in <1ms; tool dispatch overhead <1ms; no heap allocations on the hot path (message forwarding)
**Constraints**: <5MB RAM budget for embedded targets; no blocking the tokio runtime; FRB-compatible types only in public API (no `dyn Trait`, no raw pointers); single shared channel to minimize FFI overhead
**Scale/Scope**: Typical host app registers 1–20 tools; single concurrent tool execution per conversation turn; must handle agent rebuilds without tool loss

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Status | Evidence |
|---|-----------|--------|----------|
| I | Trait-Driven Modularity | ✅ PASS | Host tools implement `Tool` trait internally via `HostToolProxy` struct. No core orchestration changes — tools are injected via existing `AgentBuilder.tools()`. New `HostToolRegistry` registered in `AgentHandle` alongside `ObserverCallbackRegistry`. |
| II | Read Before Write | ✅ PASS | Plan references exact file paths: `src/api/mod.rs`, `src/api/lifecycle.rs`, `src/api/conversation.rs`, `src/api/observer.rs` (pattern), `src/tools/traits.rs`, `src/agent/agent.rs`. All read and understood before design. |
| III | Minimal Patch | ✅ PASS | Only new files: `src/api/host_tools.rs` (registry + proxy). Only modified files: `src/api/mod.rs` (add module + re-exports), `src/api/lifecycle.rs` (add registry field to AgentHandle), `src/api/conversation.rs` (inject host tools after rebuild). No speculative abstractions. |
| IV | Public API Testing | ✅ PASS | Plan requires tests for: registration/unregistration, duplicate name rejection, schema validation, tool invocation round-trip, timeout handling, rebuild persistence, cancellation. Tests cover all 14 FRs. |
| V | Security by Default | ✅ PASS | Host tools bypass autonomy checks (clarified in spec — host app is trusted). No weakening of existing security policy — built-in tool sandboxing unchanged. Schema validation prevents malformed input. Name collision check prevents shadowing built-in tools. |
| VI | Task Clarity | ✅ PASS | Tasks (in Phase 2, `/speckit.tasks`) will reference exact file paths, user stories, and dependencies. |
| VII | Performance Discipline | ✅ PASS | Registry uses `Arc<parking_lot::Mutex<HashMap>>` — zero allocations on read path. Channel uses unbounded `mpsc` (appropriate for expected scale of 1–20 tools). No new dependencies added. Feature-gated FRB wrappers behind `#[cfg(feature = "frb")]`. |

## Project Structure

### Documentation (this feature)

```text
specs/002-host-tool-registration/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
│   └── library-api.md   # Public Rust API contract
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
src/
├── api/
│   ├── mod.rs               # MODIFY: add host_tools module + re-exports
│   ├── host_tools.rs        # NEW: HostToolRegistry, HostToolProxy, registration/invocation API
│   ├── lifecycle.rs         # MODIFY: add host_tool_registry field to AgentHandle
│   ├── conversation.rs      # MODIFY: inject host tools after agent rebuild
│   └── types.rs             # MODIFY: add HostToolSpec, ToolRequest, ToolResponse types
├── tools/
│   └── traits.rs            # READ-ONLY: Tool trait reference (no changes needed)
└── agent/
    └── agent.rs             # READ-ONLY: AgentBuilder.tools() reference (no changes needed)

tests/
└── integration/
    └── api_host_tools_test.rs  # NEW: integration tests for host tool registration + invocation
```

**Structure Decision**: Single project, extending existing `src/api/` module. New file `src/api/host_tools.rs` contains the registry and proxy. Types added to `src/api/types.rs` for FRB compatibility. Integration tests in `tests/integration/api_host_tools_test.rs`.

## Complexity Tracking

No constitution violations to justify.

## Constitution Re-Check (Post-Design)

*Re-evaluated after Phase 1 design artifacts are complete.*

| # | Principle | Status | Post-Design Evidence |
|---|-----------|--------|---------------------|
| I | Trait-Driven Modularity | ✅ PASS | `HostToolProxy` implements `Tool` trait. `HostToolRegistry` follows `ObserverCallbackRegistry` pattern. New `Agent::replace_host_tools()` follows `set_observer()` precedent. No core orchestration changes. |
| II | Read Before Write | ✅ PASS | Design references exact code: `observer.rs` pattern, `AgentHandle` fields, `apply_config_changes_if_needed` rebuild flow, `AgentBuilder.tools()`, `Tool` trait signature. |
| III | Minimal Patch | ✅ PASS | 1 new file (`src/api/host_tools.rs`), 4 modified files (`mod.rs`, `lifecycle.rs`, `conversation.rs`, `types.rs`). 1 new method on Agent (`replace_host_tools`). No new crate dependencies. No speculative features. |
| IV | Public API Testing | ✅ PASS | Test plan covers: registration happy path, duplicate rejection, schema validation, invocation round-trip, timeout, rebuild persistence, cancellation, unregistration, FRB wrapper. All FRs mapped to test cases. |
| V | Security by Default | ✅ PASS | Host tools bypass autonomy checks per spec clarification (host app is trusted). No built-in tool security weakened. Schema validation prevents malformed input. Name collision check prevents shadowing. |
| VI | Task Clarity | ✅ PASS | Deferred to `/speckit.tasks` — plan provides exact file paths, FR mappings, and dependency chain for task generation. |
| VII | Performance Discipline | ✅ PASS | No new dependencies. `parking_lot::Mutex` (already a dependency via existing code) for sub-microsecond locks. `mpsc::unbounded_channel` for zero-copy message passing. Feature-gated FRB wrappers. UUID generation is the only per-invocation allocation. |
