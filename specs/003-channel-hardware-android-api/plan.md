# Implementation Plan: Optional Channel and Hardware Modules

**Branch**: `003-channel-hardware-android-api` | **Date**: 2026-03-25 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/003-channel-hardware-android-api/spec.md`

## Summary

Gate all ~30 always-compiled channel implementations behind individual Cargo feature flags (`channel-<name>`) so mobile/Android builds can use ZeroClaw as a pure API library without compiling any channel or messaging dependencies. A `channels-all` meta-feature preserves backward compatibility. The gateway module must be decoupled from specific channel types to function without channel features. Hardware gating (already optional) is preserved unchanged.

## Technical Context

**Language/Version**: Rust 1.87, edition 2021
**Primary Dependencies**: tokio (async runtime), serde/toml (config), axum (gateway), reqwest (HTTP), ring/hmac/sha2 (crypto). Channel-specific: lettre (email), async-imap, tokio-tungstenite (WebSocket), nostr-sdk, matrix-sdk, prost (protobuf), wa-rs-* (WhatsApp Web)
**Storage**: SQLite (session persistence), optional PostgreSQL (memory-postgres feature)
**Testing**: `cargo test` / `cargo nextest run`, inline `#[cfg(test)]` modules + `tests/integration/`
**Target Platform**: Linux (server/desktop), Android (via frb), macOS, ARM SoCs (RPi, STM32)
**Project Type**: Library + CLI + daemon (hybrid)
**Performance Goals**: ≥20% binary size reduction for no-channels build; ≥30% dependency count reduction
**Constraints**: Must compile with `--no-default-features --features frb`; zero breaking changes for existing default-feature users
**Scale/Scope**: ~30 channel implementations to gate, ~4600 lines in channels/mod.rs, ~40 channel source files, 6 gateway webhook routes to gate

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Trait-Driven Modularity | ✅ PASS | Feature gating uses `#[cfg]` on existing trait implementations. No new traits needed. Core `Channel` trait remains always-compiled. |
| II. Read Before Write | ✅ PASS | Thorough investigation completed: channels/mod.rs structure, gateway imports, Cargo.toml features, daemon supervision, CLI wiring, config schema all analyzed. |
| III. Minimal Patch | ✅ PASS | Each channel gets one `#[cfg]` attribute + one feature flag. No speculative abstractions. `channels-all` meta-feature is the only new convenience mechanism, driven by FR-002/FR-003. |
| IV. Public API Testing | ✅ PASS | Tests required: (1) compilation with `--no-default-features`, (2) compilation with individual channel flags, (3) config tolerance for non-compiled channels, (4) gateway without channels. |
| V. Security by Default | ✅ PASS | No security policy changes. Feature gating reduces attack surface (fewer compiled code paths). Gateway changes are HIGH risk per constitution — requires full CI validation. |
| VI. Task Clarity | ✅ PASS | Tasks will reference exact file paths (Cargo.toml, src/channels/mod.rs, src/gateway/mod.rs, src/daemon/mod.rs, src/lib.rs, src/main.rs, src/config/schema.rs). |
| VII. Performance Discipline | ✅ PASS | This feature directly serves performance: smaller binaries, fewer dependencies for mobile/constrained targets. Binary size is a measured success criterion (SC-006). |

**Gate result**: ALL PASS — proceed to Phase 0.

## Project Structure

### Documentation (this feature)

```text
specs/003-channel-hardware-android-api/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output (Cargo feature flag contract)
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (files modified by this feature)

```text
Cargo.toml                          # Feature flags + optional deps
src/
├── lib.rs                          # Conditional `pub mod channels;`
├── main.rs                         # Conditional CLI channel subcommand
├── channels/
│   └── mod.rs                      # #[cfg] gates on all channel modules + pub use + collect_configured_channels()
├── config/
│   └── schema.rs                   # No compile-time changes — config fields stay always-present per R4 (tolerant parsing)
├── daemon/
│   └── mod.rs                      # Conditional channel supervisor spawning
└── gateway/
    └── mod.rs                      # Decouple from specific channel types; feature-gate webhook routes

tests/
└── integration/
    └── feature_gating_test.rs      # Compilation tests for feature combinations (if needed)
```

**Structure Decision**: Single-project structure. All changes are edits to existing files — no new modules or directories except possibly one integration test file. The `channels/` module and `gateway/` module are the primary modification targets.

## Complexity Tracking

> No constitution violations to justify. All principles pass cleanly.
