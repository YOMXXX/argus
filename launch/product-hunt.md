# Product Hunt Launch Copy

## Product Name

```text
Argus
```

## Tagline

```text
Make your AI coding agent prove it works.
```

## Short Description

```text
Argus is an open-source black box and verification gate for AI coding agents. It records every step, runs your verification commands, and exposes reliability tools over MCP for Claude Code, Cursor, Codex, Aider, and custom agents.
```

## Categories

Primary:

- AI Coding Agents
- Developer Tools

Secondary:

- AI Agents
- Open Source
- Engineering

## Gallery Shot List

1. Hero: `Argus never blinks. Make your AI coding agent prove it works.`
2. Demo: verification gate catches a bad result, then passes after the fix.
3. Trace: JSONL black-box timeline with task, model, tool, and verify events.
4. MCP: Claude Code / Cursor / Codex connect to Argus as a verification tool.
5. Eval: repeated repo-local tasks produce a pass-rate report.

## Maker Comment

```text
AI coding agents are useful, but the scary part is not when they fail. It is when they fail and confidently say they are done.

I built Argus as a trust layer around those agents:

- verification gate: no "done" until your commands pass
- black-box trace: every model/tool/verify step is recorded
- repo-local evals: measure reliability on your own codebase
- MCP server: plug verification into Claude Code, Cursor, Codex, or any MCP-capable agent

You can try the core flow without an API key:

curl -fsSL https://raw.githubusercontent.com/YOMXXX/argus/master/install.sh \
  | ARGUS_VERSION=v0.1.1 sh

argus demo

I would love feedback from people already using coding agents in real repos. What would make an agent "trustworthy enough" for your workflow?
```

## FAQ

### Is Argus another coding agent?

It can run as a standalone agent, but the sharper use case is as a verification and trace layer for the agent you already use.

### Does it require a paid model?

No. `argus demo` uses the built-in mock provider. Real runs can use Anthropic, OpenAI-compatible APIs, OpenRouter, or local OpenAI-compatible endpoints.

### How does MCP fit?

Run `argus mcp-serve --workspace <repo>`. By default it exposes only `verify`, so existing agents can prove work before claiming success. Broader tools are opt-in.

### What is recorded?

Task events, thoughts, model requests/responses, tool calls, route decisions, verification results, and eval results are recorded in an open JSONL trace.

### Is it open source?

Yes. It is dual licensed under MIT or Apache-2.0.

## Launch-Day Schedule

All times should use the maker's local timezone.

- T-24h: confirm GitHub Release, README, demo gif, and website.
- T-12h: send private preview to supporters and ask for honest comments.
- T-1h: prepare first comment, X thread, LinkedIn post, and GitHub Discussions welcome thread.
- Launch: publish Product Hunt page.
- +15m: post maker comment.
- +30m: publish X thread.
- +1h: post LinkedIn.
- +2h: reply to every substantive comment.
- +6h: publish one technical proof post.
- +24h: publish transparent launch recap.

## What Not To Do

- Do not buy votes.
- Do not ask for blind support.
- Do not claim enterprise readiness.
- Do not compare with competitors by insulting them.
- Do not hide that the project is early.
