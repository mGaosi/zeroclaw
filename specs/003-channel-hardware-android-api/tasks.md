# Tasks: Optional Channel and Hardware Modules

**Input**: Design documents from `/specs/003-channel-hardware-android-api/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Tests for public API surface are MANDATORY per constitution Principle IV. Every phase that introduces public API functions MUST include a corresponding test task.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup (Cargo Feature Flags)

**Purpose**: Define all new feature flags and update Cargo.toml dependencies. This is the foundation — no conditional compilation can happen until features exist.

- [x] T001 Add ~26 new `channel-*` feature flags (channel-telegram, channel-discord, channel-discord-history, channel-slack, channel-mattermost, channel-signal, channel-email, channel-gmail-push, channel-irc, channel-webhook, channel-reddit, channel-bluesky, channel-twitter, channel-qq, channel-imessage, channel-wecom, channel-mochat, channel-nextcloud-talk, channel-wati, channel-whatsapp, channel-dingtalk, channel-clawdtalk, channel-notion, channel-linq, channel-cli) to `[features]` section in Cargo.toml; `channel-email` includes `["dep:lettre", "dep:mail-parser", "dep:async-imap"]`, all others `= []`
- [x] T002 Add `channels-all` meta-feature to Cargo.toml that depends on all individual `channel-*` flags plus existing `channel-nostr`, `channel-matrix`, `channel-lark`, `whatsapp-web`, `voice-wake`
- [x] T003 Update `default` feature list in Cargo.toml: replace `"channel-nostr"` with `"channels-all"` (FR-003 backward compatibility)
- [x] T004 Update `ci-all` feature list in Cargo.toml: add `"channels-all"` to preserve CI coverage (FR-012)
- [x] T005 Make `lettre`, `mail-parser`, and `async-imap` dependencies `optional = true` in `[dependencies]` section of Cargo.toml (FR-006)

**Checkpoint**: `cargo check` passes with default features (all channels still compiled). Feature flags exist but no `#[cfg]` attributes yet.

---

## Phase 2: Foundational (Core Channel Module Gating)

**Purpose**: Gate all channel module declarations and re-exports in `src/channels/mod.rs`. This is the blocking prerequisite — all user stories depend on channel modules being conditionally compiled.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [x] T006 Add `#[cfg(feature = "channel-<name>")]` to all ~26 currently-unconditional `pub mod` declarations in src/channels/mod.rs (telegram, discord, discord_history, slack, mattermost, signal, email_channel, gmail_push, irc, webhook, reddit, bluesky, twitter, qq, imessage, wecom, mochat, nextcloud_talk, wati, whatsapp, dingtalk, clawdtalk, notion, linq, cli); keep traits, session_store, session_backend, session_sqlite, transcription, tts, link_enricher always compiled (FR-005)
- [x] T007 Add `#[cfg(feature = "channel-<name>")]` to all ~26 currently-unconditional `pub use` re-export lines in src/channels/mod.rs matching each channel module gated in T006
- [x] T008 Gate all channel instantiation blocks in `collect_configured_channels()` in src/channels/mod.rs with `#[cfg(feature = "channel-<name>")]`; for each `#[cfg(not(feature = "channel-<name>"))]` block, add a `warn!()` log if the config field is `Some` but the feature is disabled (FR-010, FR-011)
- [x] T009 Gate the channel-specific dispatch arms in `start_channels()` and `doctor_channels()` in src/channels/mod.rs with matching `#[cfg(feature = "channel-<name>")]` attributes
- [x] T010 Gate channel-specific arms in `handle_command()` in src/channels/mod.rs (e.g., `BindTelegram` behind `channel-telegram`); add a fallback message for the `channel` CLI subcommand when no channels are compiled (FR-008)
- [x] T010b [US1] Write unit test in src/channels/mod.rs `#[cfg(test)]` module: `test_config_warning_for_disabled_channel` — verify that when a config field is Some but the feature is not compiled, the warning codepath is reachable (FR-011, Principle IV)
- [x] T011 Verify `cargo check --no-default-features` compiles cleanly after all channel gating (zero errors, zero warnings)

**Checkpoint**: Core library compiles with `--no-default-features`. All channel code is excluded. `cargo check` with default features also passes (all channels still included).

---

## Phase 3: User Story 1 — Mobile API-Only Build (Priority: P1) 🎯 MVP

**Goal**: A Flutter developer builds ZeroClaw with `--no-default-features --features frb` and gets a working API library with no channel code.

**Independent Test**: `cargo check --no-default-features` succeeds. `cargo test --no-default-features --lib` passes core library tests. Binary size is ≥20% smaller.

### Tests for User Story 1

- [x] T012 [US1] Run `cargo check --no-default-features --features frb` and verify zero errors and zero warnings in src/channels/, src/gateway/, src/daemon/, src/lib.rs, src/main.rs
- [x] T013 [US1] Run `cargo test --no-default-features --lib` and verify all non-channel tests pass (agent, config, memory, providers, tools, security, api)
- [x] T014 [US1] Measure binary size: build with `cargo build --release --no-default-features` and `cargo build --release`, compare sizes, verify ≥20% reduction (SC-006)
- [x] T015 [US1] Run `cargo tree --no-default-features | wc -l` vs `cargo tree | wc -l`, verify ≥30% dependency count reduction (SC-002)

### Implementation for User Story 1

- [x] T016 [P] [US1] Keep `pub mod channels;` always compiled in src/lib.rs (FR-005 requires core channel infrastructure — traits, session_store, session_backend, etc. — always available). No gating needed on the module declaration itself; individual channel sub-modules are already gated in T006–T007
- [x] T017 [P] [US1] Gate the `ChannelCommands` enum and CLI `channel` subcommand dispatch in src/main.rs behind a cfg attribute that checks if any channel feature is enabled; when no channels compiled, print a helpful message listing available channel features (FR-008)
- [x] T018 [P] [US1] Update daemon channel supervisor in src/daemon/mod.rs: wrap the `has_supervised_channels()` call and channel supervisor spawn block behind `#[cfg(any(feature = "channels-all", ...))]`; when no channels compiled, always call `health::mark_component_ok("channels")` and log info (FR-007). Also verify daemon starts with `--no-default-features` (no channels AND no gateway) and logs "no external interfaces active" warning (EC-004)
- [x] T019 [US1] Fix any remaining compilation errors or warnings in the no-default-features build by adding necessary `#[cfg]` gates to imports, helper functions, and dead code across src/ that reference channel types

**Checkpoint**: `cargo check --no-default-features` is clean. `cargo test --no-default-features --lib` passes. This is the MVP — ZeroClaw works as a pure API library.

---

## Phase 4: User Story 2 — Selective Channel Compilation (Priority: P2)

**Goal**: A server operator enables only `channel-telegram` and `channel-discord` and gets a build with only those channels.

**Independent Test**: `cargo check --no-default-features --features "channel-telegram,channel-discord,gateway"` compiles cleanly. Daemon starts with only Telegram and Discord. Config for other channels warns and skips.

### Tests for User Story 2

- [x] T020 [US2] Run `cargo check --no-default-features --features channel-telegram` — verify it compiles cleanly with no unused warnings (SC-004)
- [x] T021 [US2] Run `cargo check --no-default-features --features channel-email` — verify email channel compiles and pulls in lettre/mail-parser/async-imap via dep: syntax
- [x] T022 [US2] Run `cargo check --no-default-features --features "channel-telegram,channel-discord,gateway"` — verify selective build compiles and only includes those channels
- [x] T023 [US2] Verify `cargo check` with default features (channels-all) still compiles cleanly with zero warnings — backward compatibility (SC-003)

### Implementation for User Story 2

- [x] T025 [P] [US2] Gate gateway `use` imports for specific channel types (GmailPushChannel, WhatsAppChannel, LinqChannel, WatiChannel, NextcloudTalkChannel) in src/gateway/mod.rs behind corresponding `#[cfg(feature = "channel-*")]` attributes (FR-013)
- [x] T026 [US2] Gate gateway handler functions (handle_gmail_push_webhook, handle_whatsapp_message, handle_whatsapp_verify, handle_linq_webhook, handle_wati_verify, handle_wati_webhook, handle_nextcloud_talk_webhook) and their helper functions (whatsapp_memory_key, linq_memory_key, wati_memory_key, nextcloud_talk_memory_key) in src/gateway/mod.rs behind corresponding `#[cfg(feature = "channel-*")]` attributes
- [x] T027 [US2] Gate gateway route registrations in the Router builder in src/gateway/mod.rs: wrap `/whatsapp` routes behind `#[cfg(feature = "channel-whatsapp")]`, `/linq` behind `#[cfg(feature = "channel-linq")]`, `/wati` behind `#[cfg(feature = "channel-wati")]`, `/nextcloud-talk` behind `#[cfg(feature = "channel-nextcloud-talk")]`, `/webhook/gmail` behind `#[cfg(feature = "channel-gmail-push")]`
- [x] T028 [US2] Gate `GatewayState` struct fields for channel-specific state (gmail_push, whatsapp, linq, wati, nextcloud_talk) in src/gateway/mod.rs behind corresponding `#[cfg(feature = "channel-*")]` attributes, and update the struct initialization code to conditionally set these fields
- [x] T029 [US2] Fix any compilation errors in gateway when building with individual channel features by resolving import and type reference issues

**Checkpoint**: Any combination of channel features compiles cleanly. Gateway works with or without channel features. Config tolerance is verified.

---

## Phase 5: User Story 3 — Hardware Stays Optional (Priority: P3)

**Goal**: Verify hardware gating is preserved and consistent with the new channel gating pattern.

**Independent Test**: `cargo check --no-default-features` has no hardware deps. `cargo check --features hardware` compiles.

### Tests for User Story 3

- [x] T030 [US3] Verify `cargo tree --no-default-features | grep -i "tokio-serial\|nusb\|rppal"` returns empty — no hardware deps compiled
- [x] T031 [US3] Verify `cargo check --features hardware` compiles cleanly — hardware feature still works

### Implementation for User Story 3

- [x] T032 [US3] Review src/peripherals/ for any accidental coupling with channel modules; confirm peripheral code has no imports from src/channels/ implementation types (only traits if needed)

**Checkpoint**: Hardware gating unchanged and consistent. No new regressions.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Full validation, CI, and documentation

- [x] T033 [P] Run `cargo clippy --all-targets -- -D warnings` with default features (channels-all) and fix any warnings
- [x] T034 [P] Run `cargo clippy --all-targets --no-default-features -- -D warnings` and fix any warnings in the minimal build
- [x] T035 Run `cargo fmt --all -- --check` and fix any formatting issues
- [x] T036 Run full test suite: `cargo test` with default features — verify all existing tests pass (SC-003)
- [x] T037 Run `cargo test --no-default-features --lib` — verify core tests pass without channels
- [x] T038 [P] Measure final binary sizes and dependency counts; document results as a comment in Cargo.toml or spec notes
- [ ] T039 Run `./dev/ci.sh all` for full CI validation (HIGH risk per constitution — gateway changes)

**Checkpoint**: All validation passes. Feature is ready for PR.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies — start immediately
- **Phase 2 (Foundational)**: Depends on Phase 1 — BLOCKS all user stories
- **Phase 3 (US1 - Mobile API-Only)**: Depends on Phase 2 — MVP delivery
- **Phase 4 (US2 - Selective Channels)**: Depends on Phase 2 — can run in parallel with Phase 3
- **Phase 5 (US3 - Hardware)**: Depends on Phase 2 — can run in parallel with Phase 3 and 4
- **Phase 6 (Polish)**: Depends on Phases 3, 4, 5

### User Story Dependencies

- **US1 (P1)**: Requires Phase 2. No dependency on US2 or US3. This is the MVP.
- **US2 (P2)**: Requires Phase 2. Gateway gating is independent of US1 (different files). Can run in parallel with US1.
- **US3 (P3)**: Requires Phase 2. Pure verification — no code changes expected. Can run in parallel with US1 and US2.

### Within Each Phase

- Tasks marked [P] within a phase can run in parallel
- Unmarked tasks within a phase must run sequentially
- Tests should be run AFTER implementation tasks in the same story

### Parallel Opportunities

Within Phase 1:
- T001–T005 must be sequential (all edit Cargo.toml)

Within Phase 2:
- T006 and T007 are sequential (both edit mod.rs declarations)
- T008, T009, T010 are sequential (all edit functions in mod.rs)
- T010b runs after T010 (tests config warning introduced in T008; Principle IV)
- T011 runs after all T006–T010b

Within Phase 3 (US1):
- T016, T017, T018 can run in parallel [P] (different files: lib.rs, main.rs, daemon/mod.rs)
- T019 runs after T016–T018 (cleanup pass)
- T012–T015 (tests) run after T016–T019

Within Phase 4 (US2):
- T025 runs first (gateway imports)
- T026 runs after T025 (gateway handlers, same file)
- T027, T028 are sequential after T026
- T029 runs after T027–T028
- T020–T023 (tests) run after T025–T029

Within Phase 6:
- T033, T034, T038 can run in parallel [P]
- T035, T036, T037 are sequential
- T039 runs last

---

## Parallel Example: User Story 1

```bash
# After Phase 2 is complete, launch US1 implementation in parallel:
Task T016: Gate channels module in src/lib.rs
Task T017: Gate CLI channel subcommand in src/main.rs
Task T018: Update daemon supervisor in src/daemon/mod.rs
# Then sequential cleanup:
Task T019: Fix remaining compilation issues
# Then validate:
Task T012: cargo check --no-default-features
Task T013: cargo test --no-default-features --lib
Task T014: Binary size comparison
Task T015: Dependency count comparison
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Define feature flags in Cargo.toml
2. Complete Phase 2: Gate all channel modules in channels/mod.rs
3. Complete Phase 3: No-default-features build works
4. **STOP and VALIDATE**: `cargo check --no-default-features` is clean
5. This alone delivers the core value for mobile builds

### Incremental Delivery

1. Phase 1 + 2 → Feature flags exist, channels gated → Foundation ready
2. Phase 3 (US1) → Mobile API-only build works → MVP!
3. Phase 4 (US2) → Gateway decoupled, selective compilation → Full value
4. Phase 5 (US3) → Hardware verification → Completeness
5. Phase 6 → Polish, CI, docs → PR-ready

### Risk Notes

- **HIGH risk**: Gateway changes (Phase 4, src/gateway/mod.rs) — requires full `./dev/ci.sh all`
- **MEDIUM risk**: Channel module gating (Phase 2, src/channels/mod.rs) — large file, many conditional blocks
- **LOW risk**: Cargo.toml features (Phase 1), hardware verification (Phase 5)
