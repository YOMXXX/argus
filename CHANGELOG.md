# Changelog

## Unreleased

- Added `arguscode`, the daily Workbench entrypoint for developers; running `arguscode` opens the TUI.
- Added `arguscode init`, `arguscode status`, and `arguscode doctor`.
- Added `.argus/config.toml` generation with provider, verification, rules, memory, and UI profiles.
- Added project detection for Rust, Node, Python, and Go repositories.
- Added project memory and smoke eval generation under `.argus/`.
- Added the first ArgusCode Workbench TUI shell with project, session, trace/memory, and terminal/verify panes.
- Added Workbench command palette (`Ctrl+K`) and help overlay (`?`) for keyboard-first operation.
- Added `arguscode task` and `arguscode resume` with a local `.argus/tasks/queue.jsonl` task queue.
- Connected Workbench input to the local task queue so tasks entered in the TUI persist across sessions.
- Added `arguscode resume --run` to execute the latest queued task through the Argus harness and write a per-task trace.
- Added `arguscode verify` to run the detected project verification gate directly from the ArgusCode entrypoint.
- Added Workbench task execution state so the TUI can run the latest queued task, refresh queue status, and render harness output plus the latest trace path.
- Added provider profile management with `arguscode provider deepseek`, OpenAI-compatible base URLs, and API-key environment variable mapping.
- Added session history at `.argus/sessions/history.jsonl` plus `arguscode history` for replayable completed task records.
- Added session history rendering inside the Workbench Trace panel and a command palette shortcut to open it.
- Added real Workbench diff preview from git status/diff with a command palette refresh action.
- Added real Workbench trace timeline preview from the latest task JSONL trace.
- Added executable Workbench verification gate output in the Terminal panel.
- Added Workbench slash commands for verification, task runs, diff refresh, history, memory, and provider profile lookup.
- Added Workbench slash commands to update and persist provider/model profiles, including DeepSeek via OpenAI-compatible settings.
- Added Workbench task queue slash commands for listing, canceling, and requeueing tasks.
- Added ArgusCode security profiles for sandbox and approval mode, with Workbench slash commands and harness argument mapping.
- Added Workbench repo map scanning for codebase shape, top directories, extensions, rules, and verify commands.
- Added Workbench eval dashboard scanning for `.argus/evals/*.json` suites and cases.
- Added Workbench workflow status and an Execution Cockpit journal for queue, run, verify, review, rework, checkpoint, route, and eval events.
- Added patch-review friendly Workbench review output that filters Argus runtime metadata and lists changed files.
- Added agent-compatibility aliases, combined rule-file import, and `arguscode health` compatibility reporting.
- Added a durable Workbench planning engine with `/plan`, `/next`, and `/done` backed by `.argus/plans/current.json`.
- Added verify-failure classification and automatic Workbench repair task generation.
- Added launch readiness checks via `arguscode launch` and Workbench `/launch`.
- Updated release packaging and installer support so archives install both `argus` and `arguscode`.

## 0.1.1 - 2026-06-30

- Added step-aware trace forks with `argus trace fork --step`.
- Added reproducible eval resets, repeated sampling, `--no-gate`, and JSON eval reports.
- Added default eval isolation with temp workspaces, `--in-place` opt-in, and git worktree isolation for `reset: "git"`.
- Exposed `verify`, `eval`, and `route` through `argus mcp-serve`; default MCP server exposure is now `verify` only, with `--allow-tool eval` / `--allow-tool route` opt-in.
- Added MCP client `--mcp-allow` and require it when combining `--yes` with external MCP tools.
- Added zero-config `argus demo`, `argus doctor`, and `argus policy show`.
- Hardened file tool path checks against symlink escape, added verifier timeouts, bounded command output, and Unix process-group cleanup on command timeout.
- Added release validation scripts, checksum release archives, and checksum-verifying installer support.
