# Argus Launch Command Center

ArgusCode should launch as the open AI coding workbench and harness for developers who want one verified place to work with coding agents.

Core line:

> Do not trust your coding agent. Make it prove the work.

Primary audience:

- Developers using Claude Code, Cursor, Codex, Aider, Cline, or custom MCP agents.
- Engineering leads who want agentic coding without "fake done" risk.
- Open-source maintainers who want reproducible evals for AI-assisted changes.

Primary proof:

- `arguscode` opens the daily Workbench TUI.
- `arguscode init` detects the project, imports rules, and creates memory/config/evals.
- `arguscode task` queues work locally and `arguscode resume` picks up the latest queued task.
- Tasks typed directly in the Workbench persist to `.argus/tasks/queue.jsonl`.
- The Workbench command palette can run the latest queued task and show harness output plus the trace path.
- `arguscode resume --run` executes the latest queued task through the Argus harness and writes a per-task trace.
- `arguscode verify` runs the same detected verification gate without leaving the ArgusCode entrypoint.
- `arguscode provider deepseek` configures an OpenAI-compatible model profile while keeping API keys in environment variables.
- `arguscode history` lists completed task sessions from `.argus/sessions/history.jsonl`.
- The Workbench Trace panel renders recent session history and the latest task trace timeline.
- The Workbench Session panel renders git status/diff preview and can refresh it from the command palette.
- The Workbench Terminal panel can execute the configured verification gate from the command palette.
- The Workbench input supports slash commands for verify, run, diff, history, memory, and provider lookup.
- The Workbench can update provider/model profiles from slash commands and persist them to `.argus/config.toml`.
- The Workbench can list, cancel, and requeue tasks from slash commands without leaving the TUI.
- The Workbench can tune sandbox and approval profiles from slash commands, and harness runs pass those settings to `argus run`.
- `argus demo` shows the verification gate catching a bad result and forcing a fix.
- `argus trace show` exposes the black-box timeline.
- `argus mcp-serve` gives existing agents a verification tool without switching workflows.
- `argus eval` turns reliability into a number on the user's own repo.

## Launch Sequence

### Phase 0: Repository Readiness

Target: before public posting.

- GitHub Release `v0.1.1` is published.
- README top fold has demo, install, quick start, MCP setup, and benchmark.
- `benchmarks/demo.gif` loads in the README.
- `launch/` contains ready-to-post copy.
- `site/index.html` is ready for static hosting.
- Repo topics should be set in GitHub UI:
  - `ai-agent`
  - `coding-agent`
  - `mcp`
  - `developer-tools`
  - `rust`
  - `evals`
  - `llmops`
  - `ai-coding`

### Phase 1: Private Proof Loop

Target: 20 direct conversations before public launch.

Ask people to run exactly this:

```bash
curl -fsSL https://raw.githubusercontent.com/YOMXXX/argus/master/install.sh \
  | ARGUS_VERSION=v0.1.1 sh

argus demo
argus mcp-serve --workspace .
```

DM target segments:

- AI coding power users.
- OSS maintainers with active test suites.
- Developer-tool founders.
- Security-minded engineering leads.
- People posting about agent failures, flaky coding agents, or MCP tooling.

Ask for three things:

1. Did the one-liner make sense?
2. Did `argus demo` make the value obvious?
3. What would block you from using it in a real repo?

### Phase 2: Hacker News

Post type: `Show HN`.

Best title:

```text
Show HN: Argus - an open-source black box and verification gate for AI coding agents
```

Post from a personal account. Stay technical, direct, and humble. The post should not sound like a press release.

Launch-day conduct:

- Reply quickly for the first 4 hours.
- Answer technical criticism with concrete implementation details.
- Do not ask for upvotes.
- Do not use hype words like "revolutionary", "game-changing", or "autonomous software engineer".
- Link to exact commands and source files when asked.

### Phase 3: Product Hunt

Launch Product Hunt after the HN post has produced feedback and wording improvements.

Position as:

```text
Make your AI coding agent prove it works.
```

Do not lead with Rust, MCP, or architecture. Lead with the pain: coding agents say they are done when they are not.

### Phase 4: Sustained Distribution

The first spike is not the goal. The goal is repeated proof.

Publish one technical artifact every 2-3 days for two weeks:

- "I caught my AI coding agent saying done before tests passed."
- "How to add a verification gate to Claude Code with MCP."
- "Why AI coding agents need black-box traces."
- "Running coding-agent evals on your own repo."
- "Cheap model first, strong model only when verification fails."

## Success Metrics

Day 1:

- 100+ GitHub stars.
- 10+ real comments or issues.
- 5+ people run `argus demo`.

Week 1:

- 500+ GitHub stars.
- 25+ issues/discussions/comments from real users.
- 5+ external posts mentioning Argus.
- 3+ contributors or serious integration conversations.

Month 1:

- 2,000+ GitHub stars.
- Homebrew install path shipped.
- Hosted trace viewer or HTML trace export shipped.
- At least one case study from a real repository.

## Daily Operator Checklist

Morning:

- Check GitHub issues, discussions, and release downloads.
- Reply to comments within 12 hours.
- Turn repeated questions into README or FAQ updates.

Midday:

- Post one concrete proof artifact.
- DM 5 high-fit users with a personalized note.
- Ask one user for permission to quote their feedback.

Evening:

- Review analytics and referral sources.
- Update launch copy based on objections.
- Prepare the next day's post before sleeping.

## Assets

- Show HN: [`show-hn.md`](show-hn.md)
- Product Hunt: [`product-hunt.md`](product-hunt.md)
- Social posts: [`social-posts.md`](social-posts.md)
- Outreach: [`outreach.md`](outreach.md)
- Demo script: [`demo-script.md`](demo-script.md)
- Static landing page: [`../site/index.html`](../site/index.html)
