<!--
  Sync Impact Report
  Version change: 0.0.0 → 1.0.0 (initial ratification)
  Modified principles: N/A (first version)
  Added sections:
    - Core Principles (7 principles)
    - Security Requirements
    - Development Workflow
    - Governance
  Removed sections: N/A
  Templates requiring updates:
    - .specify/templates/tasks-template.md ✅ updated (test policy aligned)
    - .specify/templates/plan-template.md ✅ no changes needed (dynamic gates)
    - .specify/templates/spec-template.md ✅ no changes needed
  Follow-up TODOs: None
-->

# ZeroClaw Constitution

## Core Principles

### I. Trait-Driven Modularity

All extension points MUST be defined as Rust traits. New
functionality (providers, channels, tools, memory backends,
observers, runtime adapters, peripherals) MUST be added by
implementing the corresponding trait and registering in the
factory module. Direct modification of core orchestration
code is prohibited when a trait extension suffices.

**Rationale**: ZeroClaw supports 20+ providers, 15+ channels,
and hardware peripherals. Trait-driven design keeps the core
stable while enabling unlimited extension without merge
conflicts or coupling.

### II. Read Before Write

Every code change MUST be preceded by inspection of the
target module, its factory wiring, and adjacent tests. No
blind edits. When planning tasks, each task description MUST
reference the exact file path(s) to be modified and the
relevant context that was read.

**Rationale**: ZeroClaw's codebase has deep wiring between
modules (agent loop, provider routing, config schema,
observability pipeline). Changes without context risk
breaking non-obvious dependencies.

### III. Minimal Patch

Changes MUST be the smallest patch that achieves the goal.
Speculative abstractions, unused config keys, "just in case"
feature flags, and organizational-only refactors are
prohibited. Every line of diff MUST serve a concrete,
immediate purpose.

- No heavy dependencies for minor convenience.
- No speculative config/feature flags without a consumer.
- No mixing formatting-only changes with functional changes.
- No modifying unrelated modules "while here."

**Rationale**: Minimal patches are easier to review, less
likely to introduce regressions, and faster to ship. In a
high-risk, security-sensitive runtime, unnecessary complexity
is itself a defect.

### IV. Public API Testing (NON-NEGOTIABLE)

Every public API function, trait implementation, and
user-facing interface MUST have corresponding test cases that
verify correct behavior. Tests MUST be included as part of
the implementation task — they are not optional or deferred.

- Inline `#[cfg(test)]` modules for unit tests.
- `tests/` directory for integration and component tests.
- All tests MUST pass before a PR can merge.
- Tests MUST cover: happy path, validation rejection,
  error conditions, and edge cases documented in the spec.

When planning tasks, every phase that introduces public API
surface MUST include a test task. Task definitions MUST
clearly specify what is being tested and which requirements
are covered.

**Rationale**: The user requires all public APIs to have test
cases and run smoothly. ZeroClaw is an autonomous agent
runtime where untested APIs can cause silent failures in
production — on hardware devices, mobile apps, and
always-on servers.

### V. Security by Default

Security policy MUST NOT be silently weakened. All changes
to `src/security/`, `src/tools/`, `src/gateway/`, and
access-control boundaries are classified HIGH risk and
require full CI validation (`./dev/ci.sh all`).

- Workspace isolation MUST be maintained for all file ops.
- Path traversal (`..`, absolute paths) MUST be blocked.
- Command execution MUST use allowlists (Supervised mode).
- Secrets MUST NOT appear in config files, test data,
  commits, or logs.
- Autonomy levels (ReadOnly, Supervised, Full) MUST be
  enforced by the security policy module.

**Rationale**: ZeroClaw executes shell commands, reads/writes
files, and calls external APIs on behalf of the user. A
single security regression can compromise the host system.

### VI. Task Clarity

Tasks MUST be planned as clearly as possible. Each task
definition MUST include:

- A unique ID (e.g., T001, T002).
- The exact file path(s) to be created or modified.
- The user story it belongs to (e.g., [US1], [US2]).
- Whether it can run in parallel ([P] marker).
- A concise but complete description of what to implement.

Tasks MUST be assignable to a single developer or subagent
without requiring additional clarification. If a task
requires context from another task, the dependency MUST be
explicitly declared in the Dependencies section.

**Rationale**: The user requires tasks planned as clearly as
possible and assigned to subagents. Ambiguous tasks cause
wasted cycles, rework, and blocked pipelines.

### VII. Performance Discipline

ZeroClaw targets constrained devices ($10 hardware, <5MB
RAM, ARM SoCs, mobile phones). Changes MUST NOT introduce
unnecessary allocations, bloated dependencies, or
unbounded resource consumption.

- Binary size MUST be monitored when adding dependencies.
- Feature flags MUST gate heavy optional functionality
  (e.g., gateway, hardware peripherals).
- Async operations MUST use tokio primitives; no blocking
  the runtime.
- Memory and CPU impact of new features MUST be considered
  in the plan.

**Rationale**: ZeroClaw runs on Raspberry Pi, STM32 boards,
Android phones, and other resource-constrained targets.
Performance is a feature, not an afterthought.

## Security Requirements

ZeroClaw implements defense-in-depth security with three
autonomy levels (ReadOnly, Supervised, Full) and five
sandboxing layers:

1. **Workspace isolation** — All file ops confined to the
   workspace directory.
2. **Path traversal blocking** — `..` sequences and absolute
   paths rejected.
3. **Command allowlisting** — Only approved commands execute
   in Supervised mode.
4. **Forbidden path list** — Critical system paths (`/etc`,
   `/root`, `~/.ssh`) always blocked.
5. **Rate limiting** — Max actions per hour and cost per day.

Changes that touch sandboxing, policy enforcement, or
access-control boundaries MUST include security-focused
tests and MUST pass `cargo test -- security` and
`cargo test -- tools::shell`.

## Development Workflow

1. **Read before write** — Inspect target module, factory
   wiring, and tests before editing.
2. **One concern per PR** — No mixed feature+refactor+infra.
3. **Implement minimal patch** — No speculative abstractions.
4. **Validate by risk tier**:
   - Low risk (docs/tests/chore): lightweight checks.
   - Medium risk (src/** behavior): full relevant checks.
   - High risk (security/gateway/tools/runtime/CI): full
     `./dev/ci.sh all`.
5. **Document impact** — PR notes for behavior, risk, side
   effects, and rollback.
6. **Queue hygiene** — Declare `Depends on #...` or
   `Supersedes #...` as appropriate.

**Quality gates** (all PRs):
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- High-risk PRs: `./dev/ci.sh all`

**Branch rules**:
- Work from a non-`master` branch.
- Conventional commit titles.
- Small PRs preferred (size: XS/S/M).
- Never commit secrets or personal data.

## Governance

This constitution supersedes all ad-hoc practices. All PRs,
reviews, and automated checks MUST verify compliance with
the principles above. Violations MUST be flagged during
review and resolved before merge.

**Amendment procedure**:
1. Propose the change with rationale in a PR modifying this
   file.
2. Document the version bump (MAJOR for principle removals
   or redefinitions, MINOR for additions, PATCH for
   clarifications).
3. Update the `Last Amended` date.
4. Run the propagation checklist: verify `plan-template.md`,
   `spec-template.md`, `tasks-template.md`, and any command
   templates remain consistent with the amended principles.

**Compliance review**: The `/speckit.plan` command MUST
include a Constitution Check gate. The `/speckit.analyze`
command MUST verify task coverage of public API testing
(Principle IV) and task clarity (Principle VI).

**Guidance file**: `CLAUDE.md` provides runtime development
guidance (commands, repo map, risk tiers) and is
complementary to this constitution. In case of conflict,
this constitution takes precedence.

**Version**: 1.0.0 | **Ratified**: 2026-03-24 | **Last Amended**: 2026-03-24
