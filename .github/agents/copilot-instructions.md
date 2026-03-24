# zeroclaw Development Guidelines

Auto-generated from all feature plans. Last updated: 2026-03-24

## Active Technologies
- Rust stable, edition 2021 + tokio (async runtime), serde/toml (config), reqwest/rustls (HTTP), rusqlite/bundled (storage), flutter_rust_bridge 2.11 (optional FRB FFI), axum/tower/tower-http/rust-embed (gateway, optional) (001-android-port-streaming)
- SQLite (bundled, cross-platform), TOML config files (001-android-port-streaming)
- Rust (stable, edition 2021) + tokio (async runtime), serde/toml (config), reqwest+rustls (HTTP), rusqlite+bundled (storage), flutter_rust_bridge 2.11 (FFI, optional `frb` feature) (001-android-port-streaming)
- SQLite via rusqlite (bundled for Android/iOS cross-compilation) (001-android-port-streaming)

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
- 001-android-port-streaming: Added Rust (stable, edition 2021) + tokio (async runtime), serde/toml (config), reqwest+rustls (HTTP), rusqlite+bundled (storage), flutter_rust_bridge 2.11 (FFI, optional `frb` feature)
- 001-android-port-streaming: Added Rust stable, edition 2021 + tokio (async runtime), serde/toml (config), reqwest/rustls (HTTP), rusqlite/bundled (storage), flutter_rust_bridge 2.11 (optional FRB FFI), axum/tower/tower-http/rust-embed (gateway, optional)

- 001-android-port-streaming: Added Rust (stable, edition 2021) + tokio (async runtime), serde/toml (config), flutter_rust_bridge (FFI to Dart/Flutter); axum/tower/rust-embed (gateway-only, conditional)

<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
