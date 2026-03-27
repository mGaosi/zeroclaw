# Session Persistence

The API supports automatic persistence of complete conversation sessions (user messages, assistant responses, tool calls, and tool results) to disk.

## Configuration

Enable session persistence in your configuration:

```toml
[channels]
session_persistence = true
session_backend = "sqlite"   # or "jsonl"
```

Set `workspace_dir` via `ConfigPatch` at init time or runtime to control where session data is stored.

## Session Backends

- **sqlite** (default): Uses SQLite with WAL mode. Best for most use cases.
- **jsonl**: Append-only JSON Lines files. One file per session.

## Session Keys

Session keys identify distinct conversations. Valid keys contain only alphanumeric characters, hyphens (`-`), and underscores (`_`). When no key is provided, `"api_default"` is used.

## Concurrent Access Warning

**Running multiple agent instances against the same workspace directory is not supported.**

SQLite WAL mode allows safe concurrent reads, but no guarantees are made for concurrent writes from multiple processes. The JSONL backend does not support concurrent access at all.

Each workspace directory should be used by exactly one agent instance at a time. If you need multiple agents, assign each a separate workspace directory.

## File Permissions

On Unix systems, session storage files are created with mode `0600` (owner read/write only) to protect conversation data.
