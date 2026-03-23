# Quickstart: ZeroClaw Mobile SDK

This guide shows how to integrate ZeroClaw into a Flutter app via flutter_rust_bridge (FRB).

## Prerequisites

- Rust toolchain (stable) with Android/iOS targets:
  ```bash
  rustup target add aarch64-linux-android armv7-linux-androideabi aarch64-apple-ios
  ```
- Android NDK (r25+) or Xcode (14+) for iOS
- Flutter SDK (3.x+)
- `flutter_rust_bridge_codegen` CLI installed

## 1. Add ZeroClaw as a Rust Dependency

In your Flutter project's Rust crate (`rust/Cargo.toml`):

```toml
[dependencies]
zeroclaw = { path = "../path/to/zeroclaw", default-features = false }
```

> `default-features = false` excludes the gateway (HTTP server). Add `features = ["gateway"]` if you need it.

## 2. Run FRB Codegen

FRB scans the `src/api/` module and generates Dart bindings:

```bash
flutter_rust_bridge_codegen generate
```

This produces Dart classes for `StreamEvent`, `ConfigPatch`, `ObserverEventDto`, `ApiError`, and function wrappers for `init`, `sendMessage`, `updateConfig`, etc.

## 3. Initialize the Agent

```dart
import 'package:your_app/generated/zeroclaw.dart';

final handle = await ZeroClaw.init(
  configPath: null, // No config file — inject everything at runtime
  overrides: ConfigPatch(
    provider: 'openai',
    model: 'gpt-4',
    apiKey: await secureStorage.read(key: 'openai_key'),
  ),
);
```

## 4. Send a Message (Streaming)

```dart
final stream = ZeroClaw.sendMessage(
  handle: handle,
  message: 'What is the weather in Tokyo?',
);

await for (final event in stream) {
  switch (event) {
    case StreamEventChunk(:final delta):
      // Append text to chat bubble
      chatBubble.append(delta);
    case StreamEventToolCall(:final tool, :final arguments):
      // Show "Using tool: $tool..." indicator
      showToolIndicator(tool);
    case StreamEventToolResult(:final tool, :final output, :final success):
      // Show tool result
      showToolResult(tool, output, success);
    case StreamEventDone(:final fullResponse):
      // Finalize chat bubble
      chatBubble.finalize(fullResponse);
    case StreamEventError(:final message):
      // Show error
      showError(message);
  }
}
```

## 5. Update Config at Runtime

```dart
// Switch to a different model without restart
await ZeroClaw.updateConfig(
  handle: handle,
  patch: ConfigPatch(model: 'claude-3-opus'),
);
```

## 6. Register an Observer

```dart
final observerStream = ZeroClaw.registerObserver(handle: handle);
observerStream.listen((event) {
  switch (event) {
    case ObserverEventDtoLlmResponse(:final durationMs, :final inputTokens):
      analytics.track('llm_response', {
        'duration_ms': durationMs,
        'input_tokens': inputTokens,
      });
    // ... handle other events
  }
});
```

## 7. Shutdown

```dart
await ZeroClaw.shutdown(handle);
```

## Build for Android

```bash
# From the Flutter project root
flutter build apk --release
```

FRB handles the NDK cross-compilation. Ensure `ANDROID_NDK_HOME` is set.

## Build for iOS

```bash
flutter build ios --release
```

Xcode handles the iOS cross-compilation via Cargo's aarch64-apple-ios target.

## Common Issues

| Issue                              | Solution                                                              |
| ---------------------------------- | --------------------------------------------------------------------- |
| `api_key` validation error on init | Ensure the key is provided via `overrides`, not the config file       |
| Large binary size                  | Verify `default-features = false` to exclude gateway                  |
| SQLite link errors on Android      | Ensure `rusqlite` uses `features = ["bundled"]` (default in ZeroClaw) |
| OpenSSL errors on Android          | ZeroClaw uses `rustls`, not `native-tls` — should not occur           |
