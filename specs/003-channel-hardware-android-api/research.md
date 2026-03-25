# Research: Optional Channel and Hardware Modules

**Feature**: 003-channel-hardware-android-api
**Date**: 2026-03-25

## R1: Feature Flag Granularity

**Decision**: One feature flag per channel, using `channel-<name>` naming convention.

**Rationale**: Matches the existing pattern (`channel-lark`, `channel-matrix`, `channel-nostr`). Individual flags give maximum selectivity — server operators can enable exactly the channels they need. ~30 flags is manageable in Cargo.toml and aligns with how other Rust ecosystem projects handle modular channel/backend support (e.g., `sqlx` has per-database features).

**Alternatives considered**:
- Grouped families (`channels-messaging`, `channels-social`): Rejected because it couples unrelated channels (e.g., Telegram and Signal have nothing in common), and no natural grouping exists for ~30 diverse channels.
- Hybrid (individual for heavy, grouped for light): Rejected because the distinction is arbitrary and creates an inconsistent UX.

## R2: Gateway Decoupling from Channel Types

**Decision**: Feature-gate each channel-specific webhook handler and route behind its corresponding `channel-*` feature flag. Keep the generic `/webhook` route always compiled.

**Rationale**: Investigation found 6 channel-specific webhook routes in `src/gateway/mod.rs` (WhatsApp, Linq, WATI, Nextcloud Talk, Gmail Push), all registered unconditionally. Each handler imports a specific channel type (e.g., `GmailPushChannel`, `WhatsAppChannel`). The generic `/webhook` handler has zero channel coupling and is already proof that the gateway can function without channel types.

**Strategy**:
1. Wrap each channel-specific handler function in `#[cfg(feature = "channel-<name>")]`
2. Wrap corresponding route registrations in conditional blocks
3. Move channel-specific `use` imports behind the same `#[cfg]` gates
4. The `GatewayState` struct's channel fields (e.g., `gmail_push: Option<Arc<GmailPushChannel>>`) get `#[cfg]` gates
5. The base gateway (health, webhook, API endpoints) compiles without any channel feature

**Alternatives considered**:
- Generic dispatcher trait to merge 4 near-identical handlers: Rejected as out of scope (Principle III: Minimal Patch). The handlers work today; refactoring them is a separate concern. Feature-gating them in place is simpler.
- Moving handlers to their channel modules: Rejected because it would scatter gateway routing across 30+ files and break the current centralized routing architecture.

## R3: Dependency Gating Strategy

**Decision**: Make channel-exclusive dependencies `optional` in Cargo.toml and include them via `dep:` syntax in the corresponding feature flag.

**Findings**:

| Dependency | Channels | Action |
|-----------|----------|--------|
| `lettre` | email_channel | Make optional, gate behind `channel-email` |
| `mail-parser` | email_channel | Make optional, gate behind `channel-email` |
| `async-imap` | email_channel | Make optional, gate behind `channel-email` |
| `tokio-tungstenite` | discord, discord_history, dingtalk, lark, qq, slack + self_test.rs | **Keep unconditional** — shared by 6 channels + gateway testing infra |
| `prost` | lark, whatsapp_storage | Already optional ✅ (behind `channel-lark` and `whatsapp-web`) |
| `nostr-sdk` | nostr | Already optional ✅ (behind `channel-nostr`) |
| `matrix-sdk` | matrix | Already optional ✅ (behind `channel-matrix`) |
| `wa-rs-*` | whatsapp_web | Already optional ✅ (behind `whatsapp-web`) |
| `cpal` | voice_wake | Already optional ✅ (behind `voice-wake`) |

**Key insight**: Most channels (Telegram, Discord, Slack, Reddit, Twitter, Bluesky, etc.) have NO unique external dependencies — they use only `reqwest`, `serde`, `tokio`, and other base crates. Feature-gating these channels reduces compiled Rust code (fewer modules, fewer trait impls, no factory dispatch) even though they don't eliminate external deps.

**Rationale**: `tokio-tungstenite` is kept unconditional because gating it would require either (a) a `channels-websocket` meta-feature grouping 6 unrelated channels, or (b) conditional compilation of self_test.rs gateway WebSocket testing. Neither is worth the complexity for a single lightweight dep.

## R4: Config Tolerance Strategy

**Decision**: Keep all channel config fields in `ChannelsConfig` always present (no `#[cfg]` on struct fields), and log a warning at runtime if a configured channel is not compiled in.

**Rationale**: Investigation found `ChannelsConfig` has ~28 `Option<T>` fields for channel configs. If we `#[cfg]`-gate these fields, TOML parsing would fail when a config file contains a section for a non-compiled channel (serde would reject the unknown field). By keeping fields always-present, config parsing continues to work and the system can log a specific warning like "channel-email feature not enabled; ignoring email config".

**Alternatives considered**:
- `#[cfg]` on config fields + `#[serde(deny_unknown_fields)]` removal: Rejected because it silently ignores config typos too. The current approach of keeping fields present is more explicit.
- `#[serde(flatten)]` with a catch-all HashMap: Rejected as overengineered for this use case.

**Implementation**: In `collect_configured_channels()`, add compile-time `#[cfg(not(feature = "channel-<name>"))]` blocks that log a warning if the config Option is Some but the feature is disabled.

## R5: Binary Size Reduction Estimate

**Decision**: Target ≥20% binary size reduction for no-channels build (SC-006).

**Rationale**: The 30 channel implementations total ~15,000+ lines of Rust code, plus their trait impls, async state machines, and inline dependencies. Removing all channel code should eliminate significant binary bloat. The email-only deps (lettre, mail-parser, async-imap) alone add ~1-2MB to the final binary. A 20% target is conservative and achievable.

## R6: Existing Feature Flag Rename Consideration

**Decision**: Keep existing feature names unchanged — `channel-nostr`, `channel-matrix`, `channel-lark`, `whatsapp-web`, `voice-wake`.

**Rationale**: Renaming would break existing users' Cargo.toml configurations. The new channels will use `channel-<name>` convention. The existing `whatsapp-web` and `voice-wake` names are grandfathered.

**Note**: `channel-feishu` is an alias for `channel-lark` and is preserved as-is.
