# Quickstart: API Workspace Directory & Session Persistence

**Feature**: 004-api-workspace-session-persist

## Overview

This feature adds two capabilities to the ZeroClaw embedded API:

1. **Workspace directory configuration** — Callers can specify where the agent stores files (memory, sessions, workspace content) by providing a path in `ConfigPatch.workspace_dir`. Essential for Android/iOS apps where the default home directory isn't available.

2. **Session persistence** — Complete conversations (all messages, tool calls, and tool results) are automatically saved to disk and can be resumed after restarts. Session management functions allow listing, loading, and deleting sessions.

## Quick Usage

### Initialize with a custom workspace

```rust
use zeroclaw::api::{lifecycle::init, types::ConfigPatch};

let overrides = ConfigPatch {
    workspace_dir: Some("/data/user/0/com.example.app/files/zeroclaw".into()),
    provider: Some("openai".into()),
    model: Some("gpt-4".into()),
    api_key: Some("sk-...".into()),
    ..Default::default()
};

let handle = init(Some("config.toml".into()), Some(overrides)).await?;
```

### Send a message with session tracking

```rust
use zeroclaw::api::conversation::send_message;
use tokio::sync::mpsc;

let (tx, mut rx) = mpsc::channel(64);

// First message to "chat_1" — new session created
send_message(&handle, "Hello!".into(), Some("chat_1".into()), tx)?;

// Process streaming events
while let Some(event) = rx.recv().await {
    match event {
        StreamEvent::Chunk { delta } => print!("{delta}"),
        StreamEvent::Done { full_response } => break,
        StreamEvent::Error { message } => eprintln!("Error: {message}"),
        _ => {}
    }
}
```

### Resume a previous session

```rust
// On next app launch — same session key resumes from persisted history
let (tx, mut rx) = mpsc::channel(64);
send_message(&handle, "What were we talking about?".into(), Some("chat_1".into()), tx)?;
// Agent sees the full prior conversation context
```

### Manage sessions

```rust
use zeroclaw::api::conversation::{list_sessions, load_session_history, delete_session};

// List all sessions
let sessions = list_sessions(&handle)?;
for s in &sessions {
    println!("{}: {} messages, last active {}", s.key, s.message_count, s.last_activity);
}

// Load full history for a session
let history = load_session_history(&handle, "chat_1".into())?;

// Delete a session
delete_session(&handle, "old_chat".into())?;
```

## Key Design Decisions

- **Default session key**: When `session_key` is `None`, the system uses `"api_default"` so single-conversation apps work without managing keys.
- **Auto-load on first message**: When you send a message to a session key that has persisted history, the history is automatically loaded — no explicit "load" call required.
- **Non-blocking persistence**: If a disk write fails, the conversation continues. An error event is sent to inform the caller, and the failure is logged.
- **SQLite by default**: Uses the same SQLite+WAL backend as channel mode, providing atomic writes and crash safety.

## Files Modified

| File | Change |
|------|--------|
| `src/api/types.rs` | Add `workspace_dir` to `ConfigPatch`, add `SessionInfo` type |
| `src/api/lifecycle.rs` | Add workspace validation and session backend initialization to `init()`, add `session_backend` to `AgentHandle` |
| `src/api/conversation.rs` | Add `session_key` param to `send_message()`, add persistence hooks, add session management functions |
| `src/api/config.rs` | Handle `workspace_dir` in `update_config()` rebuild flow |
| `src/api/mod.rs` | Export new public items |
