# Social Launch Posts

## X Thread

Post 1:

```text
AI coding agents are useful now.

But the dangerous failure mode is not "it failed".

It is "it failed, then confidently said done."

I built ArgusCode: an open AI coding workbench and verification harness for AI coding agents.
```

Post 2:

```text
ArgusCode makes the agent prove the work:

- run the task
- record every model/tool step
- run your build/test/lint commands
- feed failures back into the agent
- only report success after verification passes
```

Post 3:

```text
The fastest way to understand it:

curl -fsSL https://raw.githubusercontent.com/YOMXXX/argus/master/install.sh \
  | ARGUS_VERSION=v0.1.1 sh

arguscode
argus demo

No API key needed.
```

Post 4:

```text
The part I am most excited about is MCP.

You do not need to switch away from Claude Code, Cursor, Codex, or your own agent.

Run Argus as a local MCP server and give your current agent a verification tool.
```

Post 5:

```text
I want coding agents to be more useful.

That means making them more auditable:

- open JSONL traces
- repo-local evals
- checksum releases
- conservative MCP defaults
- no "done" without proof

Repo: https://github.com/YOMXXX/argus
```

## Single X Post

```text
I built ArgusCode: an open AI coding workbench and verification harness for AI coding agents.

It opens as a TUI, records every model/tool/verify step, runs your tests before success, and plugs into Claude Code/Cursor/Codex over MCP.

Try it without an API key:

argus demo

https://github.com/YOMXXX/argus
```

## LinkedIn Post

```text
AI coding agents are becoming useful enough that the next problem is trust.

The failure mode I worry about is not just that an agent makes a mistake. It is that the agent makes a mistake, skips verification, and still says "done".

I built Argus as an open-source trust layer for coding agents:

- verification gate: no success claim until build/test/lint commands pass
- black-box trace: every model call, tool call, route decision, and verification result is recorded
- repo-local evals: measure pass rates on your own codebase
- MCP server: add verification to Claude Code, Cursor, Codex, or custom agents

You can try the demo without an API key:

curl -fsSL https://raw.githubusercontent.com/YOMXXX/argus/master/install.sh | ARGUS_VERSION=v0.1.1 sh
argus demo

Repo: https://github.com/YOMXXX/argus

I am looking for feedback from developers using AI coding tools in real repositories. What would make an agent trustworthy enough for your workflow?
```

## Reddit: r/rust

```text
Title: I built a Rust CLI that adds a verification gate and JSONL trace to AI coding agents

I built Argus, a Rust CLI for making AI coding agents auditable.

The core idea is simple: an agent should not be able to say "done" until the user's verification commands pass.

Argus records an open JSONL trace of the run, including task events, model calls, tool calls, route decisions, and verification results. It can run as a standalone CLI or as an MCP server that exposes verification to existing agents.

Repo: https://github.com/YOMXXX/argus

The demo runs without an API key:

argus demo

I would appreciate feedback on the Rust API boundaries, trace format, and whether the MCP defaults are conservative enough.
```

## Reddit: r/LocalLLaMA

```text
Title: Open-source verification gate for coding agents, works with OpenAI-compatible local endpoints

I built Argus, an open-source CLI that records AI coding agent runs and makes success depend on verification commands.

It supports OpenAI-compatible endpoints through `--base-url`, so it can be used with local servers as well as hosted providers.

The main thing I want to explore with this community: measuring reliability across models on real repo tasks instead of only global coding benchmarks.

Repo: https://github.com/YOMXXX/argus
```

## Reddit: r/programming

```text
Title: Argus: open-source black-box traces and verification gates for AI coding agents

ArgusCode is a terminal workbench for making AI coding agents auditable.

It records each run as JSONL, runs verification commands before reporting success, supports repo-local evals, and exposes a conservative MCP server so other coding agents can call its verification tool.

The demo is zero-config:

argus demo

Repo: https://github.com/YOMXXX/argus

I am interested in feedback on the trace format and whether this kind of verification layer belongs inside coding agents or beside them.
```

## Discord / Slack Short Blurb

```text
I just published Argus, an open-source verification gate and black-box trace for AI coding agents.

It is meant for the "agent said done but tests were not actually green" problem.

Try: `argus demo`
Repo: https://github.com/YOMXXX/argus
```

## Follow-Up Posts

### Day 2

```text
The most useful feedback so far: people do not want another coding agent. They want the agent they already use to prove its work.

That is why Argus MCP mode exposes `verify` by default.
```

### Day 4

```text
Argus traces are JSONL on purpose.

I want agent runs to be inspectable with boring tools:

cat .argus/trace.jsonl
jq .
git diff
custom dashboards

If an AI agent changed your repo, the timeline should not be trapped in a vendor UI.
```

### Day 7

```text
Global coding benchmarks are useful, but your repo is where reliability matters.

Argus evals are repo-local:

- define task
- define verify commands
- run repeated samples
- compare pass rates
- drop into CI
```
