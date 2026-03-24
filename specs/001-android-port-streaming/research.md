# Research: Mobile Port with Optional Gateway & Streaming Interface

**Date**: 2026-03-19
**Feature**: [spec.md](spec.md)

## R-001: Gateway Module Coupling Analysis

**Decision**: Gateway can be cleanly feature-gated. Deep analysis found 17 coupling points; most are **already gated**. Only 3 critical fixes remain.

**Rationale**: Full codebase grep identified every file outside `src/gateway/` referencing gateway types. The coupling is well-contained:

| # | Coupling Point | File | Status |
|---|---------------|------|--------|
| 1 | Module export | `src/lib.rs` L54 | ✅ Already gated: `#[cfg(feature = "gateway")]` |
| 2 | `GatewayCommands` enum | `src/lib.rs` L84 | ✅ Already gated |
| 3 | `Commands::Gateway` variant (use stmt) | `src/main.rs` L91 | ✅ Already gated |
| 4 | `GatewayCommands` import | `src/main.rs` L122 | ✅ Already gated |
| 5 | **`Commands::Gateway` enum variant** | **`src/main.rs` L249** | **❌ CRITICAL — NOT gated** |
| 6 | **`Commands::Gateway` match arm** | **`src/main.rs` L976** | **❌ CRITICAL — NOT gated** |
| 7 | Daemon gateway spawn | `src/daemon/mod.rs` L65-77 | ✅ Already gated |
| 8 | `node_tool` module import | `src/tools/mod.rs` L69 | ✅ Already gated at module level |
| 9 | `node_tool` registration | `src/tools/mod.rs` L146 | ✅ Already gated |
| 10 | **`NodeRegistry` import** | **`src/tools/node_tool.rs` L13** | **⚠️ Module-level gated** (safe because tools/mod.rs gates it, but L13/L35/L45/L88/L103/L152 reference gateway types) |
| 11 | Component test import | `tests/component/mod.rs` L5 | ✅ Already gated |
| 12-17 | `GatewayConfig` in schema | `src/config/schema.rs` L220, L1723 | ✅ **Keep unconditional** — config parsing should not break based on features |

**Critical fixes needed**:
1. `src/main.rs` L249: Add `#[cfg(feature = "gateway")]` on `Commands::Gateway` enum variant
2. `src/main.rs` L976: Add `#[cfg(feature = "gateway")]` on the match arm
3. Verify `src/tools/node_tool.rs` compiles correctly when `gateway` feature is disabled (module-level gate in `tools/mod.rs` should suffice)

**Alternatives considered**:
- Extracting gateway into a separate crate: rejected — too disruptive, no benefit beyond feature flags
- Making gateway a dynamically-loaded plugin: rejected — unnecessary complexity for a compile-time concern

## R-002: Agent Streaming Architecture

**Decision**: `Agent::turn_streaming()` **already exists** at `src/agent/agent.rs` L808-1045. It uses `tokio::sync::mpsc::Sender<StreamEvent>` + `CancellationToken` and emits `Chunk`, `ToolCall`, `ToolResult`, `Done`, `Error` events. No new streaming infrastructure needed — the `src/api/conversation.rs` layer wraps this existing method.

**Rationale**: Deep exploration confirmed turn_streaming() is fully implemented:
- Takes `tx: tokio::sync::mpsc::Sender<StreamEvent>`, `cancel_token: CancellationToken`, and `observer_registry` parameters
- Steps 1–5 identical to `turn()` (system prompt, memory load, auto-save, enrich, classify)
- Step 6 (tool loop): Calls `provider.chat()` **non-streaming** (not `stream_chat_with_history()` yet)
- Emits `StreamEvent::Chunk` for text, `StreamEvent::ToolCall` for tool invocations, `StreamEvent::ToolResult` for tool results
- Emits `StreamEvent::Done` when complete, `StreamEvent::Error` on failure
- Comment at L862: *"T041: Uses chat() since tool dispatch requires structured responses. Provider-level streaming (stream_chat_with_history) will be integrated when the streaming pipeline supports tool call parsing."*

**What remains**:
1. Wire `src/api/conversation.rs::send_message()` to call `agent.turn_streaming()` (the API layer exists but may need final wiring)
2. Future: integrate `stream_chat_with_history()` for true provider-level streaming (out of scope for this feature — current non-streaming approach satisfies all FRs)
3. Ensure `observer_registry` parameter is properly connected (currently stubbed per comment at L900)

**Alternatives considered**:
- Modifying `Agent::turn()` to accept a callback: rejected — less composable than a Stream, doesn't work with FRB's native Stream translation
- Using `tokio::sync::broadcast`: rejected — mpsc is simpler for single-consumer (the library API consumer)

## R-003: flutter_rust_bridge (FRB) Async Stream Compatibility

**Decision**: Define streaming API functions as `pub fn send_message(...) -> impl Stream<Item = StreamEvent>` (or return a `StreamSink`-compatible type). FRB 2.x natively translates Rust `Stream` trait objects into Dart `Stream<T>`.

**Rationale**: FRB 2.x supports:
- `Stream<Item = T>` → `Stream<T>` in Dart (automatic)
- Struct types with `#[frb]` attributes → Dart classes (automatic)
- `Result<T, E>` → Dart exceptions (automatic)
- `async fn` → Dart `Future<T>` (automatic)

The Rust API surface must:
1. Use `pub` visibility on all FRB-exposed types
2. Use serializable types (FRB handles serde-compatible structs)
3. Avoid raw pointers, `dyn Trait` objects in the public API (FRB can't translate trait objects)
4. Return concrete types, not `impl Trait` (FRB needs concrete types for codegen)

**Key constraint**: FRB expects a `StreamSink<T>` pattern for Rust-to-Dart streaming. The Rust function takes a `StreamSink<StreamEvent>` parameter and pushes events into it. This is different from returning a `Stream` directly.

```rust
// FRB-compatible pattern:
pub fn send_message(sink: StreamSink<StreamEvent>, message: String, config: AgentConfig) {
    // spawn async task that pushes events into sink
}
```

**Alternatives considered**:
- UniFFI instead of FRB: rejected — FRB has better Dart/Flutter integration and native Stream support
- Raw JNI bindings: rejected — user explicitly chose FRB

## R-004: Runtime Configuration Live Reload

**Decision**: Replace `Arc<Mutex<Config>>` with `Arc<ArcSwap<Config>>` (or keep `Arc<Mutex<Config>>` but add a `tokio::sync::watch` channel for change notifications). Subsystems subscribe to the watch channel and reinitialize when notified.

**Rationale**: The current config system:
- Loads config once at startup via `Config::load_or_init()`
- Gateway's `PUT /api/config` updates `Arc<Mutex<Config>>` in-memory + saves to disk
- No notification mechanism — other subsystems don't know config changed
- Provider/channel changes require restart

The live reload design:
1. `RuntimeConfigManager` holds `Arc<Mutex<Config>>` + `tokio::sync::watch::Sender<Config>`
2. `update_config(partial)` validates → merges → saves → sends watch notification
3. `reload_from_file()` reads TOML → validates → merges → saves → sends watch notification
4. Subsystems hold `watch::Receiver<Config>` and call `.changed().await` in their event loops
5. Provider reinitialization: create new provider instance from new config, swap atomically
6. In-flight requests: hold `Arc<dyn Provider>` from before the swap — old instance stays alive until all refs drop

**Alternatives considered**:
- File watch (inotify/kqueue): rejected — the spec calls for explicit reload trigger, not automatic
- Config diffing (only reinitialize changed subsystems): deferred — full reinit is simpler for v1

## R-005: Cargo Feature Flag Design

**Decision**: Add a `gateway` feature that gates axum, tower-http, rust-embed, mime_guess, and http-body-util as optional dependencies. Add `gateway` to the `default` features list so existing users are unaffected.

**Rationale**: Current state: 20 feature flags exist, none for gateway. Gateway deps are always compiled. The feature flag must:

```toml
[features]
default = ["gateway", "observability-prometheus", "channel-nostr", "skill-creation"]
gateway = ["dep:axum", "dep:tower", "dep:tower-http", "dep:rust-embed", "dep:mime_guess", "dep:http-body-util"]
```

Dependencies become optional:
```toml
axum = { version = "0.8", optional = true, features = [...] }
tower = { version = "0.5", optional = true }
tower-http = { version = "0.6", optional = true, features = [...] }
rust-embed = { version = "8", optional = true }
mime_guess = { version = "2", optional = true }
http-body-util = { version = "0.1", optional = true }
```

`NodeRegistry` and `NodeInvocation` in `src/tools/node_tool.rs` also need `#[cfg(feature = "gateway")]` since they depend on gateway types.

**Alternatives considered**:
- Separate `gateway` and `web-dashboard` features: rejected — over-granular for current needs
- Moving gateway to a separate crate: rejected — feature flag achieves the same with less disruption

## R-006: Android/iOS Cross-Compilation

**Decision**: The Rust library itself requires no platform-specific code for Android/iOS. The `gateway` feature disabled removes the only problematic dependencies. Cross-compilation is handled by standard Rust NDK toolchain + cargo targets.

**Rationale**: 
- ZeroClaw's core (agent, providers, memory, config, tools) uses only cross-platform crates (tokio, serde, reqwest, rusqlite)
- `rusqlite` compiles for Android/iOS via bundled SQLite (`features = ["bundled"]`)
- `reqwest` with `rustls` (not `native-tls`) avoids OpenSSL linking issues on Android
- No filesystem APIs that are Linux-specific are used in core paths
- The `src/peripherals/` module (STM32, RPi GPIO) would be excluded on mobile via separate feature flags (already gated behind `hardware`, `peripheral-rpi`)

**Key consideration**: `tokio` runtime initialization on Android must happen on a background thread (not the main/UI thread). This is handled by the Flutter side (FRB spawns Rust on an isolate thread).

**Alternatives considered**: None — standard Rust cross-compilation is the only viable approach

## R-007: Concurrent Conversation Semantics (Clarification 2026-03-23)

**Decision**: Conversations are serial within a single `AgentHandle`. The `Arc<Mutex<Agent>>` in `AgentHandle` serializes concurrent `send_message()` calls automatically — the second call queues behind the first.

**Rationale**: The existing `turn_streaming()` implementation (agent.rs L808+) takes `&mut self`, which `Arc<Mutex<Agent>>` enforces. No additional synchronization is needed. For true parallelism, the host app creates multiple `AgentHandle` instances.

**Alternatives considered**:
- Per-message queueing with explicit ordering: rejected — Mutex already provides FIFO ordering
- Rejecting concurrent calls with an error: rejected — queuing is more user-friendly and requires no host-side logic

## R-008: Config Reload Merge Semantics (Clarification 2026-03-23)

**Decision**: `reload_from_file()` uses **merge** semantics — file values overwrite matching fields; fields absent from the file retain their current in-memory values.

**Rationale**: This preserves runtime-injected secrets (FR-015). On Android, API keys are injected via `update_config()` and must survive file reloads. A full-replace approach would wipe injected secrets.

**Implementation**: Parse TOML file into a partial `Config`, then iterate over each field — if the file provides a value, overwrite; if `None`/absent, keep current. The existing `ConfigPatch::apply_to()` pattern provides the model for selective field application.

**Alternatives considered**:
- Full replace: rejected — would destroy runtime-injected secrets
- Deep merge with conflict resolution: rejected — unnecessary complexity; simple field-level overwrite suffices

## R-009: Non-Streaming Provider Fallback (Clarification 2026-03-23)

**Decision**: When a provider does not support streaming (`supports_streaming() == false`), `turn_streaming()` emits the full response as a single `StreamEvent::Chunk` followed by `StreamEvent::Done`.

**Rationale**: The existing `turn_streaming()` (agent.rs L808+) already handles this via the provider's `stream_chat_with_history()` default implementation, which returns an error chunk. The improved approach: detect `!provider.supports_streaming()`, call `provider.chat()` instead, and emit the result as `Chunk { delta: full_text }` + `Done { full_response }`.

**Alternatives considered**:
- Emitting an error event for non-streaming providers: rejected — breaks the abstraction; host app should not need to know provider capabilities
- Chunking the full response into artificial segments: rejected — misleading; single chunk is honest about granularity

## R-010: History Preservation on Provider Switch (Clarification 2026-03-23)

**Decision**: Conversation history is preserved across runtime provider switches. The common `ChatMessage` format normalizes across providers.

**Rationale**: The `Agent` struct holds `history: Vec<ChatMessage>` which uses a provider-agnostic format (role + content). When `update_config()` changes the provider, the old provider instance is dropped and a new one created, but the history vector remains in the `Agent`. The new provider receives the existing history on the next `turn()`.

**Alternatives considered**:
- Clearing history on provider switch: rejected — loses context, poor UX
- Converting history to a provider-specific format: rejected — `ChatMessage` is already the common format

## Summary

| Research Item             | Decision                                     | Risk                              |
| ------------------------- | -------------------------------------------- | --------------------------------- |
| R-001: Gateway coupling   | 17 points found, 14 already gated, 3 fixes   | Low — well-contained              |
| R-002: Streaming agent    | `turn_streaming()` already exists (L808-1045) | Low — wiring only                 |
| R-003: FRB compatibility  | `StreamSink<StreamEvent>` pattern    | Low — well-documented FRB API     |
| R-004: Live config reload | `watch` channel + subsystem reinit   | Medium — provider swap complexity |
| R-005: Feature flags      | `gateway` feature with optional deps | Low — standard Cargo pattern      |
| R-006: Cross-compilation  | No platform-specific code needed     | Low — standard Rust NDK           |
| R-007: Serial conversations | `Arc<Mutex<Agent>>` serializes calls | Low — already implemented         |
| R-008: Merge config reload  | File values overwrite, absent fields preserved | Low — mirrors ConfigPatch pattern |
| R-009: Non-streaming fallback | Single Chunk + Done for non-streaming providers | Low — graceful degradation |
| R-010: History preservation | Common ChatMessage format survives provider swap | Low — already the case |
