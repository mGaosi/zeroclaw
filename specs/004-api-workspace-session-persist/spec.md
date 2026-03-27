# Feature Specification: API Workspace Directory & Session Persistence

**Feature Branch**: `004-api-workspace-session-persist`
**Created**: 2025-01-27
**Status**: Specified
**Input**: User description: "api配置增加workspace目录配置，可以设置agent工作目录。同时让api的完整会话信息能够持久化。"

## Clarifications

### Session 2026-03-26

- Q: When the agent is initialized into a workspace with existing sessions, how should `send_message` interact with existing history? → A: Caller provides session key in `send_message`; if a persisted session exists for that key, history is auto-loaded into the agent context on the first message to that session.
- Q: Should the system set restrictive file permissions on session storage files to protect conversation data? → A: Yes — session files MUST be created with restrictive permissions (0600 on Unix) by default to prevent other users/processes from reading conversation data.
- Q: How should session persistence failures (e.g., disk full, I/O error) be reported? → A: Persistence failures MUST be reported to the caller via a `StreamEvent::Error` event and also logged at `warn` level. The conversation turn itself should still complete (persistence is non-blocking to the response).
- Q: How should the system handle two agent instances pointing to the same workspace directory concurrently? → A: Concurrent multi-instance access to the same workspace is explicitly unsupported. SQLite WAL mode provides safe concurrent reads, but no multi-writer guarantees are made. Documentation must warn against this configuration.

### Session 2026-03-27

- Q: How should session files be protected on non-Unix platforms (e.g., Windows)? → A: Unix-only `#[cfg(unix)]` enforcement with `0600` mode is sufficient. Windows deployments rely on user-profile directory isolation; no Windows ACL equivalent is required.
- Q: Should file permissions be set on every write or only at file creation? → A: Permissions should be set at file creation time only. Re-applying on every append is unnecessary overhead since the mode does not change between writes.
- Q: What quantitative threshold defines SC-003 "no user-perceptible delay"? → A: Session persistence must complete within 50 ms per turn. Exceeding this budget should be logged at warn level.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Configure Workspace Directory at Initialization (Priority: P1)

A mobile application developer using ZeroClaw through the embedded API (e.g., on Android via Flutter) needs to specify where the agent stores its working files. The platform's default home directory resolution fails (returns the process working directory), so the developer must explicitly provide a valid, writable directory path when initializing the agent.

**Why this priority**: Without a configurable workspace directory, the agent cannot reliably locate its workspace on mobile platforms. This is a prerequisite for all file-based features including session persistence, memory storage, and identity file loading.

**Independent Test**: Can be fully tested by initializing an agent with a custom workspace path and verifying that internal operations (memory, file access) use that path as their root.

**Acceptance Scenarios**:

1. **Given** a mobile app with a known writable directory, **When** the developer calls `init()` with a workspace directory path, **Then** the agent uses that path as its workspace root for all file-based operations.
2. **Given** no workspace directory is specified in `init()`, **When** the agent starts, **Then** it falls back to the existing default resolution (home directory / `.zeroclaw/workspace`).
3. **Given** the developer provides a non-existent directory path, **When** `init()` is called, **Then** the system creates the directory (and parent directories as needed) before proceeding.
4. **Given** the developer provides a path that cannot be created or is not writable, **When** `init()` is called, **Then** a clear validation error is returned.

---

### User Story 2 - Persist Complete Conversations in API Mode (Priority: P1)

A developer building a chat application wants the full conversation (user messages and assistant responses, including tool calls and results) to be automatically saved to disk so that conversations survive app restarts, process crashes, or memory pressure.

**Why this priority**: Without session persistence, all conversation history in API mode lives only in process memory. Any interruption causes permanent data loss. This is a core reliability expectation for production chat applications.

**Independent Test**: Can be tested by sending messages through the API, terminating the process, restarting, and verifying the full conversation history is recoverable.

**Acceptance Scenarios**:

1. **Given** an initialized agent with session persistence enabled, **When** a user sends a message and receives a response, **Then** both the user message and the full assistant response (including tool interactions) are persisted to the workspace.
2. **Given** a persisted conversation, **When** the app restarts and a new agent is initialized with the same workspace, **Then** the previous conversation history can be loaded and continued.
3. **Given** session persistence is enabled, **When** multiple conversations occur (identified by distinct session keys), **Then** each conversation is stored independently.
4. **Given** a long-running conversation, **When** the conversation exceeds the configured `max_history_messages`, **Then** all messages are still persisted on disk even though only the recent window is sent to the model.

---

### User Story 3 - Manage Conversation Sessions (Priority: P2)

A developer needs to manage multiple conversation threads within their application — listing available sessions, loading a previous session's history, starting a new session, or deleting old sessions.

**Why this priority**: Session management is necessary for multi-conversation applications (e.g., a chat app with conversation tabs). It builds on P1 persistence and adds the control surface needed for practical use.

**Independent Test**: Can be tested by creating several sessions, listing them, loading one by key, deleting another, and verifying the operations produce correct results.

**Acceptance Scenarios**:

1. **Given** multiple persisted sessions exist, **When** the developer requests a session list, **Then** session metadata is returned (key, creation time, last activity, message count).
2. **Given** a session key, **When** the developer requests the session history, **Then** the full ordered message history for that session is returned.
3. **Given** a session key, **When** the developer requests deletion, **Then** that session's data is removed from disk and no longer appears in listings.
4. **Given** no session key is provided when sending a message, **When** the system processes the message, **Then** a default session key (`api_default`) is used automatically.
5. *(Cross-ref FR-012)* **Given** a session key that matches an existing persisted session, **When** the first message is sent to that session, **Then** the persisted history is auto-loaded into the agent context before the turn executes.

---

### User Story 4 - Configure Workspace Directory at Runtime (Priority: P3)

A developer wants to update the workspace directory after initialization (e.g., when a user switches accounts or profiles in the app), causing the agent to re-read workspace files from the new location.

**Why this priority**: While most applications set the workspace once at startup, multi-profile or multi-tenant scenarios benefit from runtime workspace switching. This is an enhancement over the P1 init-time configuration.

**Independent Test**: Can be tested by initializing an agent, updating the workspace path via config, and verifying that subsequent operations reference the new workspace.

**Acceptance Scenarios**:

1. **Given** an initialized agent, **When** the developer updates the workspace directory via the config API, **Then** the agent rebuilds with the new workspace path.
2. **Given** a workspace directory change, **When** the agent rebuilds, **Then** active session persistence switches to the new workspace location.
3. **Given** an invalid new workspace path, **When** the developer attempts to update, **Then** the update is rejected with a validation error and the agent continues using the previous workspace.

---

### Edge Cases

- What happens when the workspace directory path contains special characters or very long paths?
- How does the system behave when the disk is full and a session write fails?
- Concurrent multi-instance access to the same workspace is unsupported; documentation must warn against it. SQLite WAL allows safe reads but not concurrent writes.
- Session persistence handles corrupted files by logging a warning and returning an empty history for that session (graceful degradation).
- Mid-write termination: SQLite WAL mode ensures atomic commits; JSONL append-only format loses at most the incomplete last line.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The `init()` function MUST accept an optional workspace directory parameter that overrides the default workspace path.
- **FR-002**: When a workspace directory is provided, the system MUST validate that the path is writable (creating it if necessary) before completing initialization.
- **FR-003**: The workspace directory setting MUST propagate to all subsystems that depend on it (session storage, memory backend, identity file loading, tool execution sandbox).
- **FR-004**: The API MUST support session persistence, storing complete conversations (all message roles and tool interactions) to the workspace directory.
- **FR-005**: Session persistence in API mode MUST use the same backend abstraction already used by channel mode, reusing the existing session storage infrastructure.
- **FR-006**: The API MUST allow callers to specify a session key when sending a message, enabling multiple independent conversation threads.
- **FR-007**: The API MUST provide functions to list sessions, load session history, and delete sessions.
- **FR-008**: Session persistence MUST be enabled by default when a workspace directory is available, consistent with channel mode behavior.
- **FR-009**: Partial writes during session persistence MUST NOT corrupt previously persisted data (append-only semantics or write-ahead protection).
- **FR-010**: The runtime config API MUST accept a workspace directory field for post-initialization workspace changes.
- **FR-011**: When the workspace directory changes at runtime, the agent MUST rebuild with the updated path, including re-initializing session storage in the new location.
- **FR-012**: When a session key is provided and a persisted session exists for that key, the system MUST auto-load the persisted history into the agent context on the first message to that session.
- **FR-013**: Session storage files MUST be created with restrictive file permissions (0600 on Unix) to protect conversation data from other users and processes. On non-Unix platforms (e.g., Windows), explicit permission enforcement is not required; the system relies on OS-level user-profile directory isolation. Permissions SHOULD be applied at file creation time, not on every write.
- **FR-014**: Session persistence failures (I/O errors, disk full) MUST be reported to the caller via an error event and logged at warn level, without blocking the conversation turn.
- **FR-015**: Concurrent multi-instance access to the same workspace directory is explicitly unsupported and MUST be documented as such.

### Key Entities

- **Session**: A conversation thread identified by a unique session key. Contains an ordered sequence of messages (user, assistant, tool call, tool result) with timestamps. Stored within the workspace's sessions directory.
- **Workspace Directory**: The root directory for all agent file operations. Contains subdirectories for sessions, memory, and other agent state. Must be a valid, writable filesystem path.
- **ConfigPatch (extended)**: The partial configuration update structure, extended with a workspace directory field to support runtime workspace changes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Developers can initialize an agent with a custom workspace directory in a single call, with no additional setup steps required.
- **SC-002**: 100% of conversation messages (all roles and tool interactions) are recoverable from disk after a process restart when session persistence is active.
- **SC-003**: Session persistence adds no user-perceptible delay (< 50 ms per turn) to message round-trips (persistence operates asynchronously or completes within the existing response pipeline).
- **SC-004**: Applications can manage at least 1,000 independent conversation sessions per workspace without performance degradation in listing or loading operations.
- **SC-005**: The API surface for session management (list, load, delete) can be called by a developer without knowledge of the underlying storage format.
- **SC-006**: Workspace directory validation at init catches 100% of unwritable paths before the agent begins processing messages.

## Assumptions

- Concurrent access from multiple agent instances to the same workspace is not a supported configuration.
- The existing session backend implementations (JSONL and SQLite) are reusable as-is for API mode; no new storage format is needed.
- The default session backend for API mode will follow the same default as channel mode (SQLite), leveraging its FTS5 search and WAL-mode durability.
- Mobile platforms (Android, iOS) will provide a valid writable directory path via their native app frameworks (e.g., `getFilesDir()` on Android).
- Session key generation for the default (no explicit key) case will use a deterministic scheme (e.g., `api_default`) rather than random UUIDs, so the same session resumes across restarts.
- The `max_history_messages` config governs how many messages are sent to the model context window, but does not limit how many messages are persisted on disk.

## Scope Boundaries

**In scope**:
- Workspace directory parameter on `init()`
- Workspace directory field in `ConfigPatch` for runtime changes
- Session persistence integration into the API conversation flow
- Session management API functions (list, load, delete)
- Reuse of existing `SessionBackend` trait and implementations

**Out of scope**:
- New session storage backends (only existing JSONL and SQLite are used)
- Session search/query API (search can be added later atop the existing FTS5 support)
- Cross-device session synchronization
- Session encryption at rest
- Identity/AIEOS fields in `ConfigPatch` (separate feature)
- Exposing session persistence configuration toggles through the API (uses config file defaults)
