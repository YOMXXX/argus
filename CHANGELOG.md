# Changelog

## 0.1.1 - 2026-06-30

- Added step-aware trace forks with `argus trace fork --step`.
- Added reproducible eval resets, repeated sampling, `--no-gate`, and JSON eval reports.
- Added default eval isolation with temp workspaces, `--in-place` opt-in, and git worktree isolation for `reset: "git"`.
- Exposed `verify`, `eval`, and `route` through `argus mcp-serve`; default MCP server exposure is now `verify` only, with `--allow-tool eval` / `--allow-tool route` opt-in.
- Added MCP client `--mcp-allow` and require it when combining `--yes` with external MCP tools.
- Added zero-config `argus demo`, `argus doctor`, and `argus policy show`.
- Hardened file tool path checks against symlink escape, added verifier timeouts, bounded command output, and Unix process-group cleanup on command timeout.
- Added release validation scripts, checksum release archives, and checksum-verifying installer support.
