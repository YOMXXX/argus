# ArgusCode Six-Block Execution Plan

**Goal:** turn ArgusCode into a model-agnostic AI coding workbench that feels familiar to Claude Code, Codex, KimiCode, MiMoCode, Aider, and Cursor users while adding Argus' trace, verification, eval, routing, and review harness.

**Architecture:** ship six product blocks as small tested commits. Keep `argus` as the scriptable harness, keep `arguscode` as the daily TUI entrypoint, and make the TUI a cockpit over durable local `.argus/` state.

## Global Constraints

- Do not build plugins or a desktop app in this phase.
- Preserve `argus` for automation and `arguscode` for the full-screen workbench.
- Keep durable workbench state under `.argus/`.
- Put new domain logic in focused modules instead of expanding `workbench.rs` by default.
- Every implementation slice must pass format, tests, clippy, diff check, and secret scan before push.

## Block 1: Execution Cockpit

Make execution feel alive and inspectable.

- Persistent `.argus/cockpit/events.jsonl` execution journal.
- TUI Terminal panel renders recent run, verify, route, eval, review, checkpoint, rollback, and rework events.
- Later slice: background runner and streaming stdout/stderr/model/tool events.

## Block 2: Interactive Patch Review

Make edits reviewable without leaving the TUI.

- File-level changed path list with staged/unstaged status.
- Focused patch summary and review actions.
- Accept/rework/rollback tied to checkpoint ids and review decisions.
- First slice: `/review` and `/patch` show reviewable changed files while filtering `.argus/` runtime metadata.

## Block 3: Agent Compatibility Layer

Make users from popular coding agents feel at home.

- Slash command aliases for Claude Code, Codex, Aider, KimiCode, and MiMoCode habits.
- Rules import normalization for `AGENTS.md`, `CLAUDE.md`, Cursor rules, and related project instructions.
- `arguscode doctor` compatibility report with migration suggestions.
- First slice: `arguscode check/health`, Workbench `/ask` and `/check`, combined multi-agent rule import, and compatibility reporting.

## Block 4: Planning Engine

Turn broad goals into executable work plans.

- `/plan <goal>` creates a durable plan with phases and acceptance gates.
- `/next` queues the next unblocked task.
- `/done` records evidence and advances the plan.
- First slice: durable `.argus/plans/current.json` plans plus Workbench `/plan`, `/next`, and `/done` commands.

## Block 5: Quality Gate And Self-Repair

Make failures productive by default.

- Failure classifier for verify/eval/harness errors.
- Automatic repair task generation with trace and failing command context.
- Eval and verify results linked back into workflow status.
- First slice: verify failures are classified and automatically queued as repair tasks from the Workbench.

## Block 6: Product Polish And Launch Readiness

Make the first `arguscode` session feel sharp.

- Stronger visual hierarchy, empty states, command palette labels, and help text.
- Updated README, changelog, demo script, benchmark notes, and launch material.
- Release checklist tied to CI, installer, docs, and demo artifacts.
