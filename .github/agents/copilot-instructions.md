# zeroclaw Development Guidelines

Auto-generated from all feature plans. Last updated: 2026-03-25

## Active Technologies
- Rust stable, edition 2021 + tokio (async runtime), serde/toml (config), reqwest/rustls (HTTP), rusqlite/bundled (storage), flutter_rust_bridge 2.11 (optional FRB FFI), axum/tower/tower-http/rust-embed (gateway, optional) (001-android-port-streaming)
- SQLite (bundled, cross-platform), TOML config files (001-android-port-streaming)
- Rust (stable, edition 2021) + tokio (async runtime), serde/toml (config), reqwest+rustls (HTTP), rusqlite+bundled (storage), flutter_rust_bridge 2.11 (FFI, optional `frb` feature) (001-android-port-streaming)
- SQLite via rusqlite (bundled for Android/iOS cross-compilation) (001-android-port-streaming)
- Rust 1.87, edition 2021 + tokio 1.50 (async runtime, mpsc channels), serde/serde_json (serialization), parking_lot (synchronous Mutex), async-trait, flutter_rust_bridge (FRB, behind `frb` feature flag) (002-host-tool-registration)
- N/A — in-memory registry only (002-host-tool-registration)
- Rust 1.87, edition 2021 + tokio (async runtime), serde/toml (config), axum (gateway), reqwest (HTTP), ring/hmac/sha2 (crypto). Channel-specific: lettre (email), async-imap, tokio-tungstenite (WebSocket), nostr-sdk, matrix-sdk, prost (protobuf), wa-rs-* (WhatsApp Web) (003-channel-hardware-android-api)
- SQLite (session persistence), optional PostgreSQL (memory-postgres feature) (003-channel-hardware-android-api)

- Rust (stable, edition 2021) + tokio (async runtime), serde/toml (config), flutter_rust_bridge (FFI to Dart/Flutter); axum/tower/rust-embed (gateway-only, conditional) (001-android-port-streaming)

## Project Structure

```text
src/
tests/
```

## Commands

cargo test; cargo clippy

## Code Style

Rust (stable, edition 2021): Follow standard conventions

## Recent Changes
- 003-channel-hardware-android-api: Added Rust 1.87, edition 2021 + tokio (async runtime), serde/toml (config), axum (gateway), reqwest (HTTP), ring/hmac/sha2 (crypto). Channel-specific: lettre (email), async-imap, tokio-tungstenite (WebSocket), nostr-sdk, matrix-sdk, prost (protobuf), wa-rs-* (WhatsApp Web)
- 002-host-tool-registration: Added Rust 1.87, edition 2021 + tokio 1.50 (async runtime, mpsc channels), serde/serde_json (serialization), parking_lot (synchronous Mutex), async-trait, flutter_rust_bridge (FRB, behind `frb` feature flag)
- 001-android-port-streaming: Added Rust (stable, edition 2021) + tokio (async runtime), serde/toml (config), reqwest+rustls (HTTP), rusqlite+bundled (storage), flutter_rust_bridge 2.11 (FFI, optional `frb` feature)


<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
