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
