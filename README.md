<div align="center">

# 👁️ ArgusCode

### *Argus never blinks.*

## The open AI coding workbench that proves it works.

**Don't trust your AI coding agent. Verify it.**

One open-source, model-agnostic harness with the best of every coding agent — Claude Code · Cursor · Codex · KimiCode · MiMoCode · Aider — **plus the one thing none of them have:** it *proves* its work with a verification gate, trace, memory, and on-repo evals. No lock-in. No black box.

`Rust` · `MIT OR Apache-2.0` · `MCP-native`

![Argus verification gate demo](benchmarks/demo.gif)

</div>

---

## Why ArgusCode?

Today's coding agents are smart — but you can't *trust* them. They drift on long tasks, claim "done" without checking, lock you to one vendor, and give you no way to prove they actually work on *your* codebase. You just... hope.

ArgusCode makes a different bet: **prove it, don't hope.** Named after the hundred-eyed guardian of Greek myth — the one who never closes all his eyes at once.

It comes in **two layers**:

1. **ArgusCode Workbench** — `arguscode` opens the full-screen TUI for daily development.
2. **Argus Harness** — `argus run`, `argus eval`, `argus trace`, and `argus mcp-serve` expose the scriptable core.

## The four things nobody else does

| | Killer feature | What it means for you |
|---|---|---|
| 🎬 | **Time-travel debugging** | Rewind any run to any step. Fork it with a different model and diff the outcomes — side by side. |
| 🛡️ | **Provable reliability** | A verification gate kills "fake done" — nothing ships until tests/build/lint pass. Plus an Eval engine that quantifies pass-rate & regressions *on your own repo*, with a CI-friendly exit code. |
| 🔌 | **Zero-migration** | `arguscode init` detects your project, imports `AGENTS.md` / `CLAUDE.md` / Cursor rules, creates memory, and keeps the old CLI harness available. |
| 💸 | **Cost-smart routing** | Cheap model first; escalate to a strong model only when verification fails — and it reports exactly what you saved. |

Everything sits on a **black-box Trace** (open JSONL): every thought, tool call, model I/O, token, route decision, and verification result — replayable, forkable, auditable.

## All-in-one — the best of every agent, none of the baggage

| You get the best of… | …without the baggage |
|---|---|
| Agent loop · tools · MCP (Claude Code) | no Anthropic lock-in, no black box |
| Smooth multi-turn flow (Cursor) | no IDE lock-in, no subscription wall |
| Open · git-friendly · model-agnostic (Aider) | **+ the verification & governance it lacks** |
| Codebase navigation `list_files` / `search_text` | structured, no shell required |
| OpenAI · OpenRouter · local Ollama | one provider abstraction, your keys |
| 🏆 time-travel · verification gate · on-repo eval · cost routing | **what no other agent has** |

## Proven on real tasks

ArgusCode doesn't just claim reliability — it **measures** it. The [reliability benchmark](benchmarks/) runs real coding tasks (fix a bug, implement a function from tests, handle edge cases) and reports a pass-rate:

```bash
./benchmarks/run-benchmark.sh --provider anthropic --model claude-sonnet-4-5 --yes
```

| Model | Pass-rate |
|---|---|
| deepseek-v4-pro | 15/15 attempts passed (100%, gate on, samples=5, 95% CI 80%–100%, 2026-06-30) |
| claude-sonnet-4-5 | _run it_ |
| gpt-4o-mini | _run it_ |
| local llama3.1 | _run it_ |

Gate-off comparison on the same run: 14/15 attempts passed (93%, 95% CI 70%–99%); one no-gate attempt failed with a provider response decoding error. Full result: [`benchmarks/results/deepseek-v4-pro-2026-06-30.md`](benchmarks/results/deepseek-v4-pro-2026-06-30.md).

Then point it at *your own* repo with `argus eval` — reliability you can put a number on.

## Built for trust

- 🦀 **Rust core** — three small crates, `arguscode` for humans and `argus` for automation.
- 🔓 **Model-agnostic** — Anthropic, OpenAI, OpenRouter, local (Ollama/vLLM/LM Studio). Your keys, your choice.
- 🧩 **MCP-native** — Argus is both an MCP *client* (consume external tools) and an MCP *server* (expose its own).
- 📜 **Open everything** — open source, open trace format, no vendor lock.

> **Status:** v0.2 foundation in progress. `arguscode` now provides the daily Workbench entrypoint, project initialization, config, memory, smoke eval generation, and a multi-pane TUI shell. The existing `argus` harness remains the reliable scriptable core.

## Install

From source:

```bash
git clone https://github.com/YOMXXX/argus && cd argus
cargo install --path crates/argus-cli     # installs `argus` and `arguscode` into ~/.cargo/bin
arguscode --help
```

From the GitHub Release:

```bash
curl -fsSL https://raw.githubusercontent.com/YOMXXX/argus/master/install.sh \
  | ARGUS_VERSION=v0.1.1 sh
```

The installer supports macOS and Linux prebuilt archives, downloads the matching `.sha256`, verifies it, then installs `argus` and `arguscode` to `${ARGUS_INSTALL_DIR:-$HOME/.local/bin}`.

Building from source requires a recent stable Rust toolchain (`rustup`/`cargo`/`rustc`).

## Share / launch kit

Want to help people discover Argus?

- Static landing page: [`site/index.html`](site/index.html)
- GitHub Pages deployment: [`.github/workflows/pages.yml`](.github/workflows/pages.yml) (enable Pages with GitHub Actions in repo settings, then run manually)
- Hacker News / Product Hunt / social copy: [`launch/`](launch/)
- The 60-second demo script: [`launch/demo-script.md`](launch/demo-script.md)

## Quick start (no API key needed)

ArgusCode ships a built-in **mock provider**, so you can see the workbench and the harness immediately — zero config:

```bash
# The daily entrypoint: initialize the current repo and open the TUI.
arguscode
# Inside the TUI, type a task and press Enter to persist it to the local queue.
# Press Ctrl+K, then run the latest queued task through the harness.
# Use /route-run [cheap] [strong] to try a cheaper model before escalating.
# The Trace panel shows recent session history and the latest task trace timeline.
# The session panel shows git status/diff preview and can refresh it from Ctrl+K.
# Use /flow to see the current queue -> run -> verify -> review -> rework state.
# The Terminal panel includes an Execution Cockpit journal for recent run/verify/review events.
# The Terminal panel can run the configured verification gate from Ctrl+K.
# The input box also supports slash commands such as /verify, /run, /route-run, /diff, and /history.
# Switch models in-session with /provider deepseek deepseek-chat or /model <name>.
# Manage the queue in-session with /tasks, /cancel <task-id>, and /retry <task-id>.
# Tune execution safety with /sandbox read-only|workspace-write|trusted and /approval auto|ask.
# Capture durable lessons with /remember <lesson> and refresh memory with /memory.
# Attach MCP tools with /mcp <server command> and /mcp-allow <tool-name>.
# Save and restore risky edits with /checkpoint [label] and /rollback [checkpoint-id].
# Review the result with /review, /accept <note>, or /rework <follow-up task>.
# Refresh codebase shape with /map for top directories, extensions, rules, and verify commands.
# Inspect eval suites with /evals, then run smoke or a suite path with /eval-run.

# Generate config, project memory, and a smoke eval without opening the TUI.
arguscode init

# Queue work for the Workbench and resume the latest queued task.
arguscode task "fix the failing parser test"
arguscode resume
arguscode resume --run
arguscode verify
arguscode history

# Use an OpenAI-compatible provider profile without writing keys to config.
arguscode provider deepseek --model deepseek-chat

# The money shot: gate catches fake done, the agent fixes, trace records it.
argus demo

# Run a task. Every step is recorded to an open JSONL trace.
argus run "add a hello-world endpoint"

# Replay the timeline from the black box.
argus trace show .argus/trace.jsonl
```

Real output:

```
$ argus run "add a hello-world endpoint"
[mock:mock] acknowledged task: add a hello-world endpoint

$ argus trace show .argus/trace.jsonl
[   0] TASK     add a hello-world endpoint
[   1] THOUGHT  Received task: add a hello-world endpoint
[   2] MODEL ->  mock (4 prompt tokens)
[   3] MODEL <-  mock (4+4 tokens):
[   4] TOOL ->   read_file({"path":"mock.txt"})
[   5] TOOL <-   read_file ok=false: error: No such file or directory
[   6] MODEL ->  mock (4 prompt tokens)
[   7] MODEL <-  mock (4+4 tokens): [mock:mock] acknowledged task: ...
```

Every step carries a monotonic `step` number — the anchor that time-travel forks from.

## Plug Argus into Claude Code / Cursor (the 30-second win)

Don't want to switch agents? Give your current one a verification gate. Argus runs *as* an MCP server — add it to your MCP config:

```json
{
  "mcpServers": {
    "argus": {
      "command": "argus",
      "args": ["mcp-serve", "--workspace", "/path/to/your/repo"]
    }
  }
}
```

By default your agent gets the `verify` tool only, so it can prove a task is actually done before claiming success. Opt into broader tools explicitly:

```json
{
  "mcpServers": {
    "argus": {
      "command": "argus",
      "args": [
        "mcp-serve", "--workspace", "/path/to/your/repo",
        "--allow-tool", "eval",
        "--allow-tool", "route"
      ]
    }
  }
}
```

## Use a real model (Anthropic / OpenAI / compatible)

```bash
export ANTHROPIC_API_KEY=sk-ant-...
argus run "explain this repo" --provider anthropic --model claude-3-5-haiku-latest
```

`--provider` defaults to `mock` (zero config). Argus also speaks the **OpenAI Chat Completions** API — so OpenAI, OpenRouter, and local servers (Ollama, vLLM, LM Studio) all work through one provider:

```bash
export OPENAI_API_KEY=sk-...
argus run "explain this repo" --provider openai --model gpt-4o-mini

# Any OpenAI-compatible endpoint via --base-url (fully offline with Ollama)
argus run "explain this repo" --provider openai --model llama3.1 \
  --base-url http://localhost:11434/v1
```

`--base-url` works on `run`, `eval`, and `route`. Same agent, same trace, your choice of model.

## The killer features, in practice

### 🛡️ Verification gate — no fake "done"

Make the agent prove it. Pass `--verify` commands; before Argus reports success, every command must exit 0 — otherwise the failure is fed back and the agent keeps fixing (with a circuit breaker):

```bash
argus run "make the failing test pass" --provider anthropic --model claude-sonnet-4-5 --yes \
  --verify "cargo build" --verify "cargo test"
```

### 🛡️ Eval — quantify reliability on your own repo

Define a suite (each case = a task + the `verify` commands that decide pass/fail), and Argus reports the pass-rate. **Exits non-zero on any failure — drop it into CI.**

```json
{ "name": "smoke",
  "cases": [ { "id": "hello", "task": "add a /hello endpoint returning 200",
               "dir": "fixtures/api", "verify": ["cargo build", "cargo test hello"],
               "reset": "git" } ] }
```
```bash
argus eval suite.json --provider anthropic --model claude-sonnet-4-5
# 1/1 passed (100%)  — each case writes its own trace under .argus/eval/

# In the ArgusCode TUI, /eval-run defaults to .argus/evals/smoke.json,
# verifies the current workspace, and writes reports/traces under .argus/eval-runs.
arguscode

# Measure stability with repeated samples and a machine-readable report.
argus eval suite.json --samples 5 --report-json .argus/eval/report.json

# Quantify the value of the verification gate: final verify still decides pass/fail.
argus eval suite.json --no-gate

# Default eval runs in isolated temp workspaces. Opt into original-directory runs explicitly.
argus eval suite.json --in-place
```
By default, each eval attempt runs in an isolated temporary workspace; `"reset": "git"` uses a temporary git worktree. Use `--in-place` only when you intentionally want the old original-directory behavior. Use `"reset": "git"` to restore a case directory before and after each sample, or `"reset": {"command": "..."}` for a custom reset command. `--samples` reports attempt pass-rate with a Wilson 95% confidence interval; `--no-gate` disables agent self-repair but keeps the final verification gate as the source of truth.

### 💸 Cost-smart routing — cheap first, escalate on failure

```bash
argus route "fix the failing test" --provider anthropic \
  --cheap claude-3-5-haiku-latest --strong claude-sonnet-4-5 --verify "cargo test"
```
```
route: escalated claude-3-5-haiku-latest → claude-sonnet-4-5 (passed)
cost: $0.0123 actual (cheap $0.0021 + strong $0.0102); vs always-strong $0.0150 → saved $0.0027
```
`--verify` is the objective signal that decides whether the cheap model succeeded. Cost is estimated from real per-model token usage; the escalation is recorded to the trace as a `ROUTE` event.

### 🎬 Time travel — fork & diff

```bash
argus trace fork .argus/a.jsonl --provider anthropic \
  --model claude-3-5-haiku-latest --out .argus/b.jsonl   # re-run the same task, different model
argus trace fork .argus/a.jsonl --step 5 --provider anthropic \
  --model claude-sonnet-4-5 --out .argus/from-step-5.jsonl
argus trace diff .argus/a.jsonl .argus/b.jsonl           # side-by-side
```

### 🔌 Zero-migration — your rules & MCP servers, as-is

```bash
# AGENTS.md / CLAUDE.md in your repo loads automatically as the system prompt
argus run "add a test for the parser" --provider anthropic --model claude-sonnet-4-5
# (loaded rules from AGENTS.md)    — or --rules <file> / --no-rules

# Connect any MCP server; allowed tools are injected behind the approval gate.
# When --yes is used with --mcp, every external tool must be allowlisted.
argus run "search the docs and summarize" --provider anthropic --model claude-sonnet-4-5 --yes \
  --mcp-allow search \
  --mcp "npx -y @modelcontextprotocol/server-everything"
```

### 🖥️ TUI — browse the black box like lazygit

```bash
argus tui                            # opens .argus/trace.jsonl
argus tui .argus/eval/hello.jsonl    # or any trace
```
Right pane = the timeline (↑/↓ or j/k to select, `q` to quit); left pane = the selected step's detail. Built on [ratatui](https://ratatui.rs).

### 🔧 Tools

Argus runs a real multi-turn loop — the model calls tools, Argus applies the sandbox policy, executes allowed tools, and feeds results back until done:

- `read_file { path }` / `write_file { path, content }` — UTF-8 files within the working directory
- `list_files { contains? }` / `search_text { pattern }` — explore the codebase (read-only, no approval needed)
- `run_shell { command }` — shell command in the working directory (**requires approval**; `--yes` to auto-approve; timed out and output-capped)
- any tools from a connected MCP server (`--mcp`)

Sandbox modes:

- `--sandbox workspace-write` (default): read/search/write workspace files; shell and MCP tools require approval.
- `--sandbox read-only`: read/search only; write, shell, and MCP tools are denied even with `--yes`.
- `--sandbox trusted`: allow all tool kinds, intended for trusted automation such as eval/route runs.
- `argus policy show --sandbox read-only`: print the exact allow/ask/deny table.
- `argus doctor`: check the local binary, git availability, and provider environment.

Current sandboxing is a workspace/policy boundary, not an OS-level container sandbox. Eval uses isolated temp workspaces by default; use containers or OS sandboxes around Argus for hostile code.

Every tool call records a `POLICY` event in the trace so approvals and denials are auditable.

## Commands

| Command | What it does |
|---|---|
| `argus run <task> [--provider P] [--model M] [--verify CMD] [--yes] [--sandbox MODE] [--base-url URL] [--rules F\|--no-rules] [--mcp "CMD"] [--mcp-allow TOOL] [--trace PATH]` | Run a task; record every step to a JSONL trace; verify completion; auto-load rules; inject allowlisted MCP tools |
| `argus trace show [PATH]` | Replay a trace as a readable timeline |
| `argus trace fork <trace> [--step N] [--provider P] [--model M] [--out PATH]` | Re-run a trace's task with a different provider/model, optionally injecting context through step `N` |
| `argus trace diff <a> <b>` | Compare two traces step by step |
| `argus eval <suite.json> [--provider P] [--model M] [--out-dir DIR] [--samples N] [--no-gate] [--report-json PATH] [--in-place]` | Batch-run an eval suite in isolated temp workspaces by default; report pass-rate; exit non-zero on any failure |
| `argus route <task> --cheap M1 --strong M2 --verify CMD [--provider P]` | Cheap model first, escalate on verification failure; report cost saved |
| `argus tui [trace]` | Browse a trace in an interactive two-pane TUI |
| `argus mcp-serve --workspace <repo> [--allow-tool eval] [--allow-tool route]` | Run Argus as an MCP server. Default exposes only `verify`; `eval` and `route` are opt-in |
| `argus demo` | Run a zero-config verification-gate demo and write a replayable trace |
| `argus doctor` | Check binary, git, MCP guidance, and provider API-key environment |
| `argus policy show [--sandbox MODE]` | Explain allow/ask/deny decisions for read/write/shell/MCP operations |

## How it works — the black box

Argus writes every run to a JSONL file (one JSON object per step). The format is open: `step`, `ts_ms`, and a tagged `kind` (`task_started` / `thought` / `model_request` / `model_response` / `tool_call` / `tool_result` / `verification_gate` / `route_decision` / `diff` / `note`). Read it with any editor, pipe it through `jq`, or replay/fork it with `argus trace`.

Three crates: `argus-trace` (the open black box) · `argus-core` (model-agnostic provider abstraction + agent loop + verifier/eval/router/MCP) · `argus-cli` (the `argus` binary). Dependency direction is one-way: `cli → core → trace`.

## Roadmap

**✅ Shipped (v1.0 feature set)** — agent loop · multi-turn tools + sandbox policy audit · approval gate · black-box trace · time-travel fork/diff · verification gate · isolated Eval engine · cost-smart routing · rules import · MCP client/server allowlists · TUI · Anthropic / OpenAI / compatible providers · release automation.

**Next**
- 📦 First public release: publish GitHub assets and checksum archives
- 🧱 Deeper sandbox isolation: container/OS sandbox profiles and command allow policies
- 🌊 Streaming output in the TUI

**Later** — drift guard · 24/7 multi-channel runs · lightweight governance · hosted option.

## Contributing

Issues and PRs welcome. The codebase is small and the tests are fast (`cargo test --workspace`); `cargo clippy --workspace --all-targets -- -D warnings` must stay clean.

## License

Dual-licensed under **MIT** or **Apache-2.0**, at your option.

---

<div align="center">
<sub>👁️ Argus never blinks.</sub>
</div>
