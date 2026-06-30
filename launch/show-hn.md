# Show HN Launch Copy

## Title

```text
Show HN: Argus - an open-source black box and verification gate for AI coding agents
```

## Post Body

```text
Hi HN,

I built Argus because coding agents are getting good enough to be useful, but not trustworthy enough to be left unverified.

The failure mode I kept seeing was simple: the agent says "done", but tests were not run, the wrong file changed, or the final state is not reproducible.

Argus is an open-source CLI that adds four things around AI coding agents:

- a verification gate: the agent does not get to claim success until your build/test/lint commands pass
- a black-box trace: every task, model call, tool call, route decision, and verification result is recorded as JSONL
- repo-local evals: run repeated coding tasks against your own repo and get pass-rate reports
- MCP mode: expose Argus verification to Claude Code, Cursor, Codex, or any MCP-capable agent without switching tools

Install:

curl -fsSL https://raw.githubusercontent.com/YOMXXX/argus/master/install.sh \
  | ARGUS_VERSION=v0.1.1 sh

No API key needed for the demo:

argus demo

The demo intentionally starts with a bad result, lets the verification gate catch it, then fixes it and writes a trace.

Repo:
https://github.com/YOMXXX/argus

I would especially like feedback on the MCP shape, the trace format, and what a useful "agent reliability benchmark" should look like on real projects.
```

## First Comment

```text
A few implementation details for people who want to inspect the internals:

- Rust workspace, one `argus` binary
- trace format is JSONL in `argus-trace`
- `argus mcp-serve --workspace <repo>` exposes `verify` by default
- `eval` and `route` are opt-in MCP tools via `--allow-tool`
- evals default to isolated temp workspaces; in-place runs require `--in-place`
- installer verifies SHA-256 checksums from GitHub Releases

The part I care most about is making "agent said done" auditable. If an agent uses Argus as a tool, the user can inspect the actual timeline instead of trusting a final summary.
```

## Reply Bank

### "How is this different from just running tests?"

```text
Running tests is the core primitive. Argus packages it into an agent loop and records the whole attempt: task, model calls, tool calls, verification failures, retries, and final result. The point is not to replace tests; it is to make tests a required gate before the agent can claim success.
```

### "Why not just use Claude Code / Cursor / Codex?"

```text
You can. The MCP mode is designed for that. Argus can run as a verification layer for the agent you already like instead of asking you to switch tools.
```

### "Is this production-ready?"

```text
It is early, but the release path is real: prebuilt binaries, checksum installer, tests, clippy, release CI, isolated eval workspaces, and conservative MCP defaults. I would treat it as ready for experimentation and integration work, not as a finished enterprise product.
```

### "What data leaves my machine?"

```text
If you use the mock provider, nothing model-related leaves. If you configure a real provider, prompts go to that provider. Traces are local JSONL files unless you share them. MCP server tools are scoped to the configured workspace.
```

### "Why JSONL traces?"

```text
Append-only JSONL keeps the trace easy to inspect, diff, stream, and replay. I wanted the format to be boring enough that people can write their own tools around it.
```

### "What should evals measure?"

```text
The most useful evals are repo-specific: small tasks with objective verify commands. I do not think one global coding benchmark is enough. Argus is meant to let teams build evals that match their actual codebase.
```

## Conduct Rules

- Do not ask anyone to upvote.
- Do not paste the same reply repeatedly.
- Do not argue about tool preference; position Argus as a trust layer.
- Thank people for specific bug reports.
- Convert good criticism into issues within 24 hours.
