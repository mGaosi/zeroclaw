# Quickstart: Building ZeroClaw with Optional Channels

## API-Only Build (Mobile / Minimal)

```bash
# Build with no channels, no gateway, no hardware
cargo build --no-default-features --features frb

# Or for a non-Flutter minimal build:
cargo build --no-default-features
```

This produces the smallest possible binary, suitable for Android/iOS integration via Flutter Rust Bridge.

## Selective Channel Build

```bash
# Only Telegram and Discord
cargo build --no-default-features --features "channel-telegram,channel-discord,gateway"

# Only Email
cargo build --no-default-features --features "channel-email,gateway"
```

## Full Build (Default — Backward Compatible)

```bash
# Same as before: all channels compiled
cargo build
```

Equivalent to:
```bash
cargo build --features "channels-all,observability-prometheus,skill-creation,gateway"
```

## Verifying What's Compiled

```bash
# Check which features are active
cargo tree --features channel-telegram -e features

# Compare binary sizes
ls -la target/release/zeroclaw  # full build
ls -la target/release/zeroclaw  # minimal build (after --no-default-features)

# Count dependencies
cargo tree --no-default-features | wc -l  # minimal
cargo tree | wc -l                        # full
```

## Adding a New Channel

When contributing a new channel implementation:

1. Create `src/channels/<name>.rs` implementing the `Channel` trait
2. Add `channel-<name> = [<optional-deps>]` to Cargo.toml features
3. Add `channel-<name>` to the `channels-all` list
4. Add `#[cfg(feature = "channel-<name>")] pub mod <name>;` in `src/channels/mod.rs`
5. Add `#[cfg(feature = "channel-<name>")] pub use <name>::<Name>Channel;` for re-export
6. Gate the factory dispatch in `collect_configured_channels()` with `#[cfg(feature = "channel-<name>")]`
7. If the channel has gateway webhook routes, gate them with `#[cfg(feature = "channel-<name>")]`
