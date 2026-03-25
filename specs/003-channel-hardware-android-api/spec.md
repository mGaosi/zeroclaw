# Feature Specification: Optional Channel and Hardware Modules

**Feature Branch**: `003-channel-hardware-android-api`
**Created**: 2026-03-25
**Status**: Implemented
**Input**: User description: "channel和hardware功能改为可选项，android等平台直接通过api交互"

## Clarifications

### Session 2026-03-25

- Q: Should channels use one feature flag per channel (~30 flags) or grouped families (~6-8 flags)? → A: One flag per channel, matching existing `channel-*` naming pattern (e.g., `channel-telegram`, `channel-discord`).
- Q: Gateway module currently imports specific channel types (GmailPushChannel, LinqChannel, WatiChannel, etc.) — should gateway work without any channel features enabled? → A: Yes. Gateway must depend only on core channel infrastructure (traits), not specific implementations. Channel-specific gateway webhook routes must be feature-gated behind the corresponding channel flag.
- Q: What quantifies "meaningfully smaller" binary size for a no-channels build? → A: ≥20% binary size reduction compared to a full `channels-all` build.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Android/Mobile App Uses ZeroClaw as Pure Library (Priority: P1) 🎯 MVP

A Flutter developer building a mobile app embeds ZeroClaw as a library. The app communicates with the agent exclusively through the library API (`init`, `send_message`, `register_tool`, etc.) and does not need Telegram, Discord, email, or any other messaging channel. The developer compiles ZeroClaw without any channel or hardware dependencies, producing a smaller binary with fewer transitive dependencies — critical for mobile APK size budgets and faster CI builds.

**Why this priority**: This is the core motivation. Android/iOS apps interact through the API layer; channels and peripherals are dead code on mobile. Making them optional directly reduces binary size, compile time, and attack surface for the primary mobile use case.

**Independent Test**: Build ZeroClaw with `--no-default-features` (plus only the features needed for API usage, e.g., `frb`). Verify it compiles, the library API works (init, send_message, register_tool, submit_tool_response), and no channel or hardware code is included in the binary.

**Acceptance Scenarios**:

1. **Given** a Cargo.toml that does not enable any channel feature, **When** the crate is compiled, **Then** no channel-related code (Telegram, Discord, Slack, etc.) is compiled and no channel-specific dependencies are linked.
2. **Given** a mobile app using only the library API, **When** the app interacts with ZeroClaw via `send_message` and host tool registration, **Then** all conversational functionality works without any channel module loaded.
3. **Given** a build with channels disabled, **When** the binary size is measured, **Then** it is at least 20% smaller than a full build with all channels enabled.
4. **Given** a build with channels disabled, **When** `cargo check` runs, **Then** there are zero compilation errors and zero warnings.

---

### User Story 2 - Server Operator Enables Only Needed Channels (Priority: P2)

A self-hosted server operator runs ZeroClaw on a VPS and only needs Telegram and Discord. The operator does not want email, Slack, IRC, or other channel code compiled in. The operator selects exactly the channels they need via Cargo features, reducing binary size and dependency surface.

**Why this priority**: Server operators benefit from selective compilation but already tolerate larger binaries. This story extends the mobile-first motivation to desktop/server targets.

**Independent Test**: Build with specific channel features enabled. Verify only those channels are available. Verify other channels are absent from the binary.

**Acceptance Scenarios**:

1. **Given** a build with only one channel feature enabled, **When** the daemon starts, **Then** only that channel can be configured and started.
2. **Given** a build with no channel features, **When** the daemon starts in API-only mode, **Then** the daemon runs without error and serves the gateway API.
3. **Given** a configuration that references a channel not compiled in, **When** the daemon starts, **Then** a clear warning message is logged indicating the channel requires a specific feature flag.

---

### User Story 3 - Hardware Peripheral Stays Optional (Priority: P3)

A developer on a non-embedded platform (desktop, server, mobile) never needs serial/USB peripheral support. The `hardware` and `peripheral-rpi` features remain off by default. When a hardware developer needs STM32 or RPi GPIO support, they explicitly enable the feature.

**Why this priority**: Hardware is already feature-gated (`hardware`, `peripheral-rpi`). This story ensures the existing gating is preserved and consistent with the new channel gating approach.

**Independent Test**: Build with `--no-default-features`. Verify no hardware dependencies (tokio-serial, nusb, rppal) are compiled. Enable `hardware` feature and verify serial peripherals work.

**Acceptance Scenarios**:

1. **Given** a default build, **When** dependencies are inspected, **Then** no hardware-specific dependencies (tokio-serial, nusb, rppal) are included.
2. **Given** a build with `hardware` enabled, **When** a serial peripheral is configured, **Then** it connects and functions normally.

---

### Edge Cases

- What happens when a user's config file references a channel that isn't compiled in? The system MUST log a clear warning naming the missing feature flag and skip the channel gracefully — no panic, no hard error.
- What happens when the CLI `channel` subcommand is used but no channels are compiled? The system MUST show a helpful message explaining which feature flags to enable.
- What happens when `--no-default-features` is used without explicitly enabling any features? The core library (config loading, agent loop, memory, providers, tools) MUST still compile and function for API-only usage.
- What happens to the daemon when no channels and no gateway are enabled? The daemon MUST still run (agent remains accessible via library API for embedded use), but it MUST log a warning that no external interfaces are active.
- What happens to existing builds that rely on the current default feature set? The default feature set MUST continue to include the same channels currently compiled — no breaking change for users who don't change their Cargo.toml.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: All currently always-compiled channel implementations (Telegram, Discord, Slack, Mattermost, Signal, Email/Gmail, IRC, Webhook, Reddit, Bluesky, Twitter, QQ, iMessage, WeCom, Mochat, Nextcloud Talk, WATI, WhatsApp Cloud, DingTalk, ClawdTalk, Notion) MUST each be gated behind an individual Cargo feature flag following the `channel-<name>` naming convention (e.g., `channel-telegram`, `channel-discord`, `channel-email`). *(Note: LinkedIn was removed — no linkedin module exists in the codebase.)*
- **FR-002**: A meta-feature `channels-all` MUST be provided that enables all channel implementations at once, for users who want the current all-inclusive behavior.
- **FR-003**: The `default` feature set MUST include `channels-all` so that existing users experience no change in behavior unless they opt out.
- **FR-004**: The core library (config, agent loop, memory, providers, tools, library API) MUST compile and function correctly when no channel features are enabled.
- **FR-005**: The `channels` module's core infrastructure (traits, session store, session backend, transcription, TTS, link enricher) MUST remain always-compiled, as it defines shared interfaces that may be used by the API layer or future extensions.
- **FR-006**: Each channel's external dependencies MUST be made optional in Cargo.toml and gated behind the corresponding channel feature flag.
- **FR-007**: The `daemon` module MUST gracefully handle the absence of channel code — if no channels are compiled, channel supervisor logic is skipped.
- **FR-008**: The CLI `channel` subcommand and `ChannelCommands` enum MUST be conditionally compiled or produce helpful messages when no channel features are enabled.
- **FR-009**: The `hardware` and `peripheral-rpi` features MUST remain as they currently are (already optional, not in default). No changes required to hardware gating.
- **FR-010**: The `start_channels()` function in the channels module MUST be conditionally compiled to exclude channel instantiation code for channels whose feature is not enabled.
- **FR-011**: Configuration parsing MUST remain tolerant — a config file with channel sections for channels not compiled in MUST NOT cause a parse error. The system MUST log a warning and ignore the section.
- **FR-012**: The `ci-all` meta-feature MUST be updated to include `channels-all` (or all individual channel features) to preserve CI coverage.
- **FR-013**: The `gateway` module MUST depend only on core channel infrastructure (traits, `ChannelMessage`, `SendMessage`) — NOT on specific channel implementation types. Channel-specific gateway webhook routes (e.g., Gmail push, WATI, WhatsApp Cloud callbacks) MUST be feature-gated behind the corresponding `channel-*` flag so the gateway compiles and serves the base API without any channel features enabled.

### Key Entities

- **Channel Feature Flag**: A Cargo feature (e.g., `channel-telegram`, `channel-discord`) that gates compilation of a specific channel implementation and its dependencies.
- **Meta-feature `channels-all`**: A convenience feature that depends on all individual channel features, providing the current all-inclusive behavior.
- **Core Channel Infrastructure**: The always-compiled subset of the channels module (traits, session store, session backend) that defines shared interfaces.

## Assumptions

- The existing feature-gated channels (Matrix, Lark/Feishu, Nostr, WhatsApp Web) serve as the reference pattern for how to gate the remaining channels.
- Channel-specific dependencies that are currently always-compiled (e.g., `lettre` for email, `async-imap`) will be made optional via `dep:` syntax.
- Some dependencies are shared across multiple channels (e.g., `tokio-tungstenite` for WebSocket-based channels). These shared deps will be pulled in by whichever channel features need them.
- The `frb` feature (Flutter Rust Bridge) is independent of channels and will continue to work with or without channel features.
- Each channel gets its own feature flag (`channel-<name>`). No grouping — this matches the existing convention and gives maximum selectivity.
- The `gateway` module will be refactored to remove direct imports of specific channel types; channel-specific webhook routes will be conditionally compiled behind the corresponding channel feature flag.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A build with `--no-default-features` compiles successfully and passes all core library tests. *(Note: `--features frb` has pre-existing flutter_rust_bridge compatibility issues unrelated to channel gating; frb validation is tracked separately.)*
- **SC-002**: A minimal build (no channels, no hardware, no gateway) reduces compiled dependency count by at least 30% compared to a full `channels-all` build.
- **SC-003**: Existing builds using default features continue to compile and pass all tests with no changes to user Cargo.toml files.
- **SC-004**: Each channel can be independently enabled and the resulting build compiles cleanly with no unused-import or dead-code warnings.
- **SC-005**: A config file referencing a non-compiled channel produces a clear warning log and the system starts normally, skipping the unconfigured channel.
- **SC-006**: A no-channels build produces a binary at least 20% smaller (in bytes) than a full `channels-all` build.
