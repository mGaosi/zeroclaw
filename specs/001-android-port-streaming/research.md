# Research: Mobile Port with Optional Gateway & Streaming Interface

**Date**: 2026-03-19
**Feature**: [spec.md](spec.md)

## R-001: Gateway Module Coupling Analysis

**Decision**: Gateway can be cleanly feature-gated with surgical `#[cfg(feature = "gateway")]` guards at 6 specific coupling points.

**Rationale**: The exploration identified every file outside `src/gateway/` that references it. The coupling is well-contained:

| Coupling Point                  | File                         | Fix                                                                                                                |
| ------------------------------- | ---------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Module export                   | `src/lib.rs` L51             | `#[cfg(feature = "gateway")] pub mod gateway;`                                                                     |
| `GatewayCommands` enum          | `src/lib.rs` L84–129         | `#[cfg(feature = "gateway")]` on the enum                                                                          |
| CLI command dispatch            | `src/main.rs` L963–1054      | `#[cfg(feature = "gateway")]` on the `Commands::Gateway` variant + match arm                                       |
| Daemon supervisor               | `src/daemon/mod.rs` L66–75   | `#[cfg(feature = "gateway")]` on the gateway spawn block                                                           |
| NodeRegistry import             | `src/tools/node_tool.rs` L11 | `#[cfg(feature = "gateway")]` on the import + node tool registration                                               |
| Config schema (`GatewayConfig`) | `src/config/schema.rs`       | **Keep unconditional** — config parsing should not break based on features; unused sections are harmlessly ignored |

**Alternatives considered**:
- Extracting gateway into a separate crate: rejected — too disruptive, no benefit beyond feature flags
- Making gateway a dynamically-loaded plugin: rejected — unnecessary complexity for a compile-time concern

## R-002: Agent Streaming Architecture

**Decision**: Wrap `Agent::turn()` into a new `Agent::turn_streaming()` that returns `Pin<Box<dyn Stream<Item = StreamEvent>>>` using `tokio::sync::mpsc` internally.

**Rationale**: The current `Agent::turn()` follows this loop:
1. Build messages from history
2. Call `provider.chat()` (non-streaming) → get `ChatResponse` with text + tool_calls
3. If tool_calls exist: execute tools, append results, loop back to step 2
4. If no tool_calls: return final text

The streaming version wraps this loop, emitting events at each stage:
- After provider response: emit `StreamEvent::Chunk { text }` for any text content
- Before tool execution: emit `StreamEvent::ToolCall { name, arguments }`
- After tool execution: emit `StreamEvent::ToolResult { name, output, success }`
- After final text (no more tool calls): emit `StreamEvent::Done { full_response }`

The provider already has `stream_chat_with_history()` returning `BoxStream<StreamChunk>` — for providers that support streaming, chunks can be forwarded directly. For non-streaming providers, the full text is emitted as a single chunk.

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

## Summary

| Research Item             | Decision                             | Risk                              |
| ------------------------- | ------------------------------------ | --------------------------------- |
| R-001: Gateway coupling   | 6 surgical `#[cfg]` guards           | Low — well-contained              |
| R-002: Streaming agent    | `turn_streaming()` + mpsc → Stream   | Medium — core loop change         |
| R-003: FRB compatibility  | `StreamSink<StreamEvent>` pattern    | Low — well-documented FRB API     |
| R-004: Live config reload | `watch` channel + subsystem reinit   | Medium — provider swap complexity |
| R-005: Feature flags      | `gateway` feature with optional deps | Low — standard Cargo pattern      |
| R-006: Cross-compilation  | No platform-specific code needed     | Low — standard Rust NDK           |
