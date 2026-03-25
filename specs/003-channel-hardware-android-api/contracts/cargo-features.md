# Cargo Feature Flag Contract

**Feature**: 003-channel-hardware-android-api
**Date**: 2026-03-25

This contract defines the public interface for ZeroClaw's optional channel feature flags as consumed by downstream crates and build configurations.

## Feature Flag Naming Convention

```
channel-<name>     # Individual channel: channel-telegram, channel-discord, ...
channels-all       # Meta: enables all individual channel flags
```

Existing flags that predate this convention are grandfathered:
- `whatsapp-web` (not `channel-whatsapp-web`)
- `voice-wake` (not `channel-voice-wake`)
- `channel-feishu` (alias for `channel-lark`)

## Default Features

```toml
default = ["observability-prometheus", "channels-all", "skill-creation", "gateway"]
```

**Backward compatibility guarantee**: Users who do not modify their Cargo.toml get the same channels as before. The only change is that `channel-nostr` moves from `default` to inside `channels-all`.

## Minimal Build Contract

The following MUST compile and function with `--no-default-features`:

| Component | Status | Notes |
|-----------|--------|-------|
| Config loading & parsing | ✅ Available | All channel config fields remain in schema |
| Agent orchestration loop | ✅ Available | Core agent logic, no channel dependency |
| Provider communication | ✅ Available | All model providers work |
| Memory backends | ✅ Available | Markdown + SQLite |
| Tool execution | ✅ Available | Shell, file, memory, browser tools |
| Library API (init, send_message, register_tool) | ✅ Available | Primary mobile use case |
| Channel modules | ❌ Not compiled | No channel code in binary |
| Gateway server | ❌ Not compiled | Requires `gateway` feature |
| Hardware peripherals | ❌ Not compiled | Requires `hardware` feature |

## Feature + Dependency Mapping

Each channel feature enables its exclusive optional dependencies:

```toml
# New feature flags (this PR)
channel-telegram = []                    # No exclusive deps
channel-discord = []                     # Uses shared tokio-tungstenite
channel-email = ["dep:lettre", "dep:mail-parser", "dep:async-imap"]
channel-gmail-push = []                  # No exclusive deps
# ... (see data-model.md for complete table)

# Existing flags (unchanged)
channel-nostr = ["dep:nostr-sdk"]
channel-matrix = ["dep:matrix-sdk"]
channel-lark = ["dep:prost"]
whatsapp-web = ["dep:wa-rs", "dep:wa-rs-core", ...]
voice-wake = ["dep:cpal"]
```

## Gateway Route Availability

Gateway routes are conditionally registered based on channel features:

| Route | Requires Feature | Available Without Feature? |
|-------|-----------------|---------------------------|
| `POST /webhook` | None (generic) | ✅ Always |
| `GET /health` | None | ✅ Always |
| `POST /api/v1/chat` | None | ✅ Always |
| `WS /ws` | None | ✅ Always |
| `POST /whatsapp`, `GET /whatsapp` | `channel-whatsapp` | ❌ 404 |
| `POST /linq` | `channel-linq` | ❌ 404 |
| `GET /wati`, `POST /wati` | `channel-wati` | ❌ 404 |
| `POST /nextcloud-talk` | `channel-nextcloud-talk` | ❌ 404 |
| `POST /webhook/gmail` | `channel-gmail-push` | ❌ 404 |

## Config Tolerance Contract

```toml
# This config is VALID even without channel-telegram compiled:
[channels_config]
cli = true

[channels_config.telegram]
token = "..."
```

**Behavior**: Config parses successfully. At channel startup, warns:
```
WARN channel-telegram feature not enabled; ignoring telegram configuration
```

## CI Feature Matrix

```toml
ci-all = [
    "channels-all",
    "hardware", "memory-postgres", "observability-prometheus",
    "observability-otel", "peripheral-rpi", "browser-native",
    "sandbox-landlock", "sandbox-bubblewrap", "probe",
    "rag-pdf", "skill-creation", "plugins-wasm"
]
```
