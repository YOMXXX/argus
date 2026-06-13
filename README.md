<div align="center">

# 👁️ Argus

### *Argus never blinks.*

## The all-in-one AI coding agent that proves it works.

**Don't trust your AI coding agent. Verify it.**

One open-source, model-agnostic tool with the best of every coding agent — Claude Code · Cursor · Codex · Aider — **plus the one thing none of them have:** it *proves* its work with a verification gate and on-repo evals. No lock-in. No black box.

`Rust` · `MIT OR Apache-2.0` · `MCP-native`

<!-- Demo gif: run `vhs benchmarks/demo.tape` to produce demo.gif, then embed it here:  ![Argus demo](benchmarks/demo.gif) -->

</div>

---

## Why Argus?

Today's coding agents are smart — but you can't *trust* them. They drift on long tasks, claim "done" without checking, lock you to one vendor, and give you no way to prove they actually work on *your* codebase. You just... hope.

Argus makes a different bet: **prove it, don't hope.** Named after the hundred-eyed guardian of Greek myth — the one who never closes all his eyes at once.

It comes in **two shapes**:

1. **A standalone agent** — `argus run "..."` runs the full think→tool→verify loop, recording everything.
2. **A trust layer for *other* agents** — `argus mcp-serve` exposes Argus's reliability tools over MCP, so Claude Code / Cursor / Codex can call them. Don't switch agents — give the one you have a verification gate and a black box.

## The four things nobody else does

| | Killer feature | What it means for you |
|---|---|---|
| 🎬 | **Time-travel debugging** | Rewind any run to any step. Fork it with a different model and diff the outcomes — side by side. |
| 🛡️ | **Provable reliability** | A verification gate kills "fake done" — nothing ships until tests/build/lint pass. Plus an Eval engine that quantifies pass-rate & regressions *on your own repo*, with a CI-friendly exit code. |
| 🔌 | **Zero-migration** | Your existing `AGENTS.md` / `CLAUDE.md` rules load automatically; connect any MCP server and its tools drop in. No rewrite. |
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

Argus doesn't just claim reliability — it **measures** it. The [reliability benchmark](benchmarks/) runs real coding tasks (fix a bug, implement a function from tests, handle edge cases) and reports a pass-rate:

```bash
./benchmarks/run-benchmark.sh --provider anthropic --model claude-sonnet-4-5 --yes
```

| Model | Pass-rate |
|---|---|
| claude-sonnet-4-5 | _run it_ |
| gpt-4o-mini | _run it_ |
| local llama3.1 | _run it_ |

Then point it at *your own* repo with `argus eval` — reliability you can put a number on.

## Built for trust

- 🦀 **Rust core** — three small crates, one `argus` binary, no runtime.
- 🔓 **Model-agnostic** — Anthropic, OpenAI, OpenRouter, local (Ollama/vLLM/LM Studio). Your keys, your choice.
- 🧩 **MCP-native** — Argus is both an MCP *client* (consume external tools) and an MCP *server* (expose its own).
- 📜 **Open everything** — open source, open trace format, no vendor lock.

> **Status:** v1.0 feature-complete (all four killer features + multi-provider + TUI + MCP server work today, with 70+ tests and zero clippy warnings). Prebuilt binaries / `curl | sh` installer / crates.io release are the next step — for now, build from source (30 seconds below).

## Install

```bash
git clone https://github.com/YOMXXX/argus && cd argus
cargo install --path crates/argus-cli     # installs `argus` into ~/.cargo/bin
argus --help
```

Requires a recent stable Rust toolchain (`rustup`/`cargo`/`rustc`).

## Quick start (no API key needed)

Argus ships a built-in **mock provider**, so you can see the whole agent loop and the black box immediately — zero config:

```bash
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
    "argus": { "command": "argus", "args": ["mcp-serve"] }
  }
}
```

Now your agent has a `verify` tool: it can *prove* a task is actually done (build/test/lint all exit 0) instead of just claiming it. Argus is the trust layer for the agents you already use.

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
               "dir": "fixtures/api", "verify": ["cargo build", "cargo test hello"] } ] }
```
```bash
argus eval suite.json --provider anthropic --model claude-sonnet-4-5
# 1/1 passed (100%)  — each case writes its own trace under .argus/eval/
```
*(MVP runs each case in place without auto-reset — keep fixtures clean between runs; results are a single-run snapshot.)*

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
argus trace diff .argus/a.jsonl .argus/b.jsonl           # side-by-side
```

### 🔌 Zero-migration — your rules & MCP servers, as-is

```bash
# AGENTS.md / CLAUDE.md in your repo loads automatically as the system prompt
argus run "add a test for the parser" --provider anthropic --model claude-sonnet-4-5
# (loaded rules from AGENTS.md)    — or --rules <file> / --no-rules

# Connect any MCP server; its tools are injected (behind the approval gate)
argus run "search the docs and summarize" --provider anthropic --model claude-sonnet-4-5 --yes \
  --mcp "npx -y @modelcontextprotocol/server-everything"
```

### 🖥️ TUI — browse the black box like lazygit

```bash
argus tui                            # opens .argus/trace.jsonl
argus tui .argus/eval/hello.jsonl    # or any trace
```
Right pane = the timeline (↑/↓ or j/k to select, `q` to quit); left pane = the selected step's detail. Built on [ratatui](https://ratatui.rs).

### 🔧 Tools

Argus runs a real multi-turn loop — the model calls tools, Argus executes them and feeds results back until done:

- `read_file { path }` / `write_file { path, content }` — UTF-8 files within the working directory
- `list_files { contains? }` / `search_text { pattern }` — explore the codebase (read-only, no approval needed)
- `run_shell { command }` — shell command in the working directory (**requires approval**; `--yes` to auto-approve)
- any tools from a connected MCP server (`--mcp`)

## Commands

| Command | What it does |
|---|---|
| `argus run <task> [--provider P] [--model M] [--verify CMD] [--yes] [--base-url URL] [--rules F\|--no-rules] [--mcp "CMD"] [--trace PATH]` | Run a task; record every step to a JSONL trace; verify completion; auto-load rules; inject MCP tools |
| `argus trace show [PATH]` | Replay a trace as a readable timeline |
| `argus trace fork <trace> [--provider P] [--model M] [--out PATH]` | Re-run a trace's task with a different provider/model |
| `argus trace diff <a> <b>` | Compare two traces step by step |
| `argus eval <suite.json> [--provider P] [--model M] [--out-dir DIR]` | Batch-run an eval suite; report pass-rate; exit non-zero on any failure |
| `argus route <task> --cheap M1 --strong M2 --verify CMD [--provider P]` | Cheap model first, escalate on verification failure; report cost saved |
| `argus tui [trace]` | Browse a trace in an interactive two-pane TUI |
| `argus mcp-serve` | Run Argus as an MCP server (exposes `verify` to Claude Code / Cursor / any MCP host) |

## How it works — the black box

Argus writes every run to a JSONL file (one JSON object per step). The format is open: `step`, `ts_ms`, and a tagged `kind` (`task_started` / `thought` / `model_request` / `model_response` / `tool_call` / `tool_result` / `verification_gate` / `route_decision` / `diff` / `note`). Read it with any editor, pipe it through `jq`, or replay/fork it with `argus trace`.

Three crates: `argus-trace` (the open black box) · `argus-core` (model-agnostic provider abstraction + agent loop + verifier/eval/router/MCP) · `argus-cli` (the `argus` binary). Dependency direction is one-way: `cli → core → trace`.

## Roadmap

**✅ Shipped (v1.0 feature set)** — agent loop · multi-turn tools + approval gate · black-box trace · time-travel fork/diff · verification gate · Eval engine · cost-smart routing · rules import · MCP client + server · TUI · Anthropic / OpenAI / compatible providers.

**Next**
- 📦 Release engineering: prebuilt binaries, `curl | sh` installer, crates.io
- 🧱 Hardened tool sandbox
- 🧰 More `mcp-serve` tools (expose `eval` / `route` to host agents)
- 🌊 Streaming output in the TUI
- 🔁 Eval with repeated sampling (statistical pass-rate, not single-snapshot)

**Later** — drift guard · 24/7 multi-channel runs · lightweight governance · hosted option.

## Contributing

Issues and PRs welcome. The codebase is small and the tests are fast (`cargo test --workspace`); `cargo clippy --workspace --all-targets -- -D warnings` must stay clean.

## License

Dual-licensed under **MIT** or **Apache-2.0**, at your option.

---

<div align="center">
<sub>👁️ Argus never blinks.</sub>
</div>
