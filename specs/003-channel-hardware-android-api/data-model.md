# Data Model: Optional Channel and Hardware Modules

**Feature**: 003-channel-hardware-android-api
**Date**: 2026-03-25

## Entity: Channel Feature Flag

A Cargo feature that gates compilation of one channel implementation and its exclusive dependencies.

### Attributes

| Field | Type | Description |
|-------|------|-------------|
| name | string | Cargo feature name, format: `channel-<name>` |
| module_path | string | Rust module path: `crate::channels::<name>` |
| channel_struct | string | Exported type: `<Name>Channel` |
| exclusive_deps | list[string] | Dependencies used ONLY by this channel (made optional) |
| shared_deps | list[string] | Dependencies shared with other channels/core (remain unconditional) |
| config_field | string | Field name in `ChannelsConfig` struct |
| gateway_routes | list[string] | HTTP routes in gateway that require this channel (if any) |
| existing_flag | bool | Whether this feature flag already exists in Cargo.toml |

### Instances (complete channel inventory)

| Feature Flag | Module | Struct | Exclusive Deps | Config Field | Gateway Routes | Existing? |
|-------------|--------|--------|---------------|-------------|----------------|-----------|
| `channel-telegram` | telegram | TelegramChannel | — | telegram | — | No |
| `channel-discord` | discord | DiscordChannel | — | discord | — | No |
| `channel-discord-history` | discord_history | DiscordHistoryChannel | — | discord_history | — | No |
| `channel-slack` | slack | SlackChannel | — | slack | — | No |
| `channel-mattermost` | mattermost | MattermostChannel | — | mattermost | — | No |
| `channel-signal` | signal | SignalChannel | — | signal | — | No |
| `channel-email` | email_channel | EmailChannel | lettre, mail-parser, async-imap | email | — | No |
| `channel-gmail-push` | gmail_push | GmailPushChannel | — | gmail_push | `/webhook/gmail` | No |
| `channel-irc` | irc | IrcChannel | — | irc | — | No |
| `channel-webhook` | webhook | WebhookChannel | — | webhook | — | No |
| `channel-reddit` | reddit | RedditChannel | — | reddit | — | No |
| `channel-bluesky` | bluesky | BlueskyChannel | — | bluesky | — | No |
| `channel-twitter` | twitter | TwitterChannel | — | twitter | — | No |
| `channel-qq` | qq | QQChannel | — | qq | — | No |
| `channel-imessage` | imessage | IMessageChannel | — | imessage | — | No |
| `channel-wecom` | wecom | WeComChannel | — | wecom | — | No |
| `channel-mochat` | mochat | MochatChannel | — | mochat | — | No |
| `channel-nextcloud-talk` | nextcloud_talk | NextcloudTalkChannel | — | nextcloud_talk | `/nextcloud-talk` | No |
| `channel-wati` | wati | WatiChannel | — | wati | `/wati` | No |
| `channel-whatsapp` | whatsapp | WhatsAppChannel | — | whatsapp | `/whatsapp` | No |
| `channel-dingtalk` | dingtalk | DingTalkChannel | — | dingtalk | — | No |
| `channel-clawdtalk` | clawdtalk | ClawdTalkChannel | — | clawdtalk | — | No |
| `channel-notion` | notion | NotionChannel | — | notion | — | No |
| `channel-linq` | linq | LinqChannel | — | linq | `/linq` | No |
| `channel-cli` | cli | CliChannel | — | cli | — | No |
| `channel-nostr` | nostr | NostrChannel | nostr-sdk | nostr | — | Yes |
| `channel-matrix` | matrix | MatrixChannel | matrix-sdk | matrix | — | Yes |
| `channel-lark` | lark | LarkChannel | prost | lark, feishu | — | Yes |
| `whatsapp-web` | whatsapp_web | WhatsAppWebChannel | wa-rs-*, serde-big-array, qrcode | — | — | Yes |
| *(whatsapp-web)* | whatsapp_storage | *(utility — not a channel)* | — | — | — | Yes (gated under `whatsapp-web`) |
| `voice-wake` | voice_wake | VoiceWakeChannel | cpal | voice_wake | — | Yes |

## Entity: Meta-Feature `channels-all`

A convenience Cargo feature that enables all individual channel feature flags.

### Definition

```toml
channels-all = [
    "channel-telegram", "channel-discord", "channel-discord-history",
    "channel-slack", "channel-mattermost", "channel-signal",
    "channel-email", "channel-gmail-push", "channel-irc",
    "channel-webhook", "channel-reddit", "channel-bluesky",
    "channel-twitter", "channel-qq", "channel-imessage",
    "channel-wecom", "channel-mochat", "channel-nextcloud-talk",
    "channel-wati", "channel-whatsapp", "channel-dingtalk",
    "channel-clawdtalk", "channel-notion", "channel-linq",
    "channel-cli",
    # Already-existing flags (included for completeness):
    "channel-nostr", "channel-matrix", "channel-lark",
    "whatsapp-web",
    # voice-wake excluded: requires libasound2-dev (system library not universally available)
]
```

### Rules

- `default` feature set includes `channels-all` (FR-003, backward compatibility)
- `ci-all` feature set includes `channels-all` (FR-012)
- Each flag independently enables its channel without requiring other flags

## Entity: Core Channel Infrastructure

Always-compiled subset of the channels module.

### Components (never feature-gated)

| Module | Purpose |
|--------|---------|
| `traits` | `Channel` trait, `SendMessage`, `ChannelMessage` types |
| `session_store` | In-memory session management |
| `session_backend` | `SessionBackend` trait |
| `session_sqlite` | SQLite session persistence |
| `transcription` | Audio-to-text shared infra |
| `tts` | Text-to-speech shared infra |
| `link_enricher` | URL preview/metadata enrichment |

## State Transitions

This feature has no runtime state transitions. The gating is purely compile-time:

```
Cargo.toml feature selection
    → #[cfg] attribute evaluation at compile time
    → Channel code included/excluded from binary
    → Runtime: collect_configured_channels() returns only compiled channels
    → Runtime: Config for non-compiled channels logs warning and is skipped
```

## Validation Rules

1. If `ChannelsConfig.{field}` is `Some(config)` but the corresponding `channel-*` feature is not compiled, the system MUST log `warn!("channel-{name} feature not enabled; ignoring {name} configuration")`
2. If no channel features are enabled and daemon starts, system MUST log `info!("No channels compiled; running in API-only mode")`
3. `channels-all` MUST always expand to every individual channel flag — adding a new channel requires updating `channels-all`
