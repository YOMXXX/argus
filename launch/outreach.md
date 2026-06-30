# Outreach Plan

The goal is not to spam. The goal is to find developers who already feel the pain and ask for specific feedback.

## Target Segments

1. AI coding power users who post about Claude Code, Cursor, Codex, Aider, Cline, or MCP.
2. OSS maintainers with active CI and visible contributor workflows.
3. Developer-tool founders building in AI infrastructure.
4. Engineering leads talking about AI-assisted development risk.
5. Security and compliance engineers interested in audit trails.

## Qualification Signals

High-fit people have at least one of these signals:

- They have complained about agents skipping tests.
- They have posted agent-generated PRs.
- They maintain repos with strong test suites.
- They build or use MCP tools.
- They discuss evals, LLM observability, or AI coding reliability.

## DM Template: AI Coding Power User

```text
Hey <name>, I saw your post about <specific agent/workflow>.

I just shipped Argus, an open-source verification gate and black-box trace for AI coding agents. It is meant for the "agent said done but tests were not actually green" problem.

The zero-config demo is:

argus demo

Repo: https://github.com/YOMXXX/argus

Would you be willing to spend 3 minutes on the demo and tell me whether the value is obvious? I am especially looking for blunt feedback from people already using coding agents in real repos.
```

## DM Template: OSS Maintainer

```text
Hey <name>, I maintain a small open-source tool called Argus.

It adds a verification gate and JSONL trace around AI coding agents, so success depends on the repo's own build/test/lint commands.

I am trying to understand whether repo-local evals would be useful for maintainers who receive or create AI-assisted changes.

Would you be open to giving feedback on the idea? No integration ask. The quick demo is `argus demo`.

Repo: https://github.com/YOMXXX/argus
```

## DM Template: MCP Builder

```text
Hey <name>, I noticed you are working with MCP.

I just shipped Argus, which can run as an MCP server for verification. By default it exposes only `verify`, so Claude Code/Cursor/Codex-style agents can prove work before claiming success.

I would value feedback on whether the MCP tool shape is right:

argus mcp-serve --workspace <repo>

Repo: https://github.com/YOMXXX/argus
```

## Email Template

Subject:

```text
Quick feedback request: verification gate for AI coding agents
```

Body:

```text
Hi <name>,

I built Argus, an open-source CLI that adds a verification gate and black-box trace to AI coding agents.

The problem: agents increasingly say "done" before a repo's actual verification commands pass.

Argus makes success depend on commands like `cargo test`, `npm test`, or `pytest`, and records the full run as local JSONL. It can also run as an MCP server so existing agents can call its verification tool.

Repo: https://github.com/YOMXXX/argus

The no-key demo is:

argus demo

If you have 3 minutes, I would appreciate blunt feedback on whether the positioning is clear and whether this would fit your workflow.

Thanks,
<your name>
```

## Tracking Table

Copy this into a private spreadsheet or issue:

| Name | Segment | Link | Contacted | Response | Feedback | Follow-up |
|---|---|---|---|---|---|---|
|  | AI coding power user |  |  |  |  |  |
|  | OSS maintainer |  |  |  |  |  |
|  | MCP builder |  |  |  |  |  |

## Rules

- Personalize the first sentence every time.
- Ask for feedback, not promotion.
- Do not ask for stars in cold outreach.
- Stop after one follow-up if there is no response.
- Turn repeated objections into README/FAQ improvements.
