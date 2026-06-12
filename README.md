<div align="center">

# 👁️ Argus

### *Argus never blinks.*

**The open-source, model-agnostic AI coding agent you can actually trust in production.**

No lock-in. No drift. Fully auditable.

</div>

---

> ⚠️ **Status: early development.** Argus is being built in the open. The design is locked; the code is on its way. Star to follow along.

## Why Argus?

Today's coding agents are smart — but you can't *trust* them. They drift on long tasks, claim "done" without checking, lock you to one vendor, and give you no way to prove they actually work on *your* codebase. You just... hope.

Argus is built on a different bet: **provable reliability and zero lock-in.** It's named after the hundred-eyed guardian of Greek myth — the one who never closes all his eyes at once.

## The four things nobody else does

| | Killer feature | What it means for you |
|---|---|---|
| 🎬 | **Time-travel debugging** | Rewind any run to any step. Fork it with a different model or prompt and diff the outcomes — side by side. |
| 🛡️ | **Provable reliability** | A verification gate kills "fake done" — nothing ships until tests/build/lint pass. Plus an Eval engine that quantifies pass-rate & regressions *on your own repo*. |
| 🔌 | **Zero-migration** | One command imports your existing skills, MCP servers, and rules (`AGENTS.md` / `CLAUDE.md`) from Claude Code / Cursor / Codex. Defect for free. |
| 💸 | **Cost-smart routing** | Cheap models for grunt work, strong models for hard problems — automatically. Cut token spend by up to ~70%. |

All of it sits on a **black-box Trace** (open JSONL) that records every thought, tool call, model I/O, token, and diff — replayable, forkable, auditable.

## Built for trust

- 🦀 **Rust core** — single static binary, zero-dependency `curl | sh` install.
- 🔓 **Model-agnostic** — Anthropic, OpenAI, Google, local, OpenRouter. Your keys, your choice.
- 🧩 **Open ecosystem** — native MCP, drop-in skills, your existing project rules.
- 📜 **Open everything** — open source, open trace format, no vendor lock.

## Install

> Phase 0 — build from source. Prebuilt binaries and a `curl | sh` installer land with v1.0.

```bash
git clone https://github.com/yourusername/argus argus && cd argus
cargo build --release
# binary at target/release/argus
./target/release/argus --help
```

Requires a recent stable Rust toolchain (`rustup`/`cargo`/`rustc`).

## Quick start

No API key needed — Phase 0 ships a built-in **mock provider**, so you can see the whole agent loop and the black-box trace immediately:

```bash
# Run a task. Every step is recorded to an open JSONL trace.
argus run "add a hello-world endpoint"

# Replay the timeline from the black box.
argus trace show .argus/trace.jsonl
```

Example output:

```
$ argus run "add a hello-world endpoint"
[mock:mock] acknowledged task: add a hello-world endpoint

$ argus trace show .argus/trace.jsonl
[   0] THOUGHT  Received task: add a hello-world endpoint
[   1] MODEL ->  mock (4 prompt tokens)
[   2] MODEL <-  mock (7 tokens): [mock:mock] acknowledged task: ...
```

Each step carries a monotonic `step` number — the anchor that time-travel debugging will fork from (Phase 1).

### Use a real model (Anthropic)

```bash
export ANTHROPIC_API_KEY=sk-ant-...
argus run "explain this repo" --provider anthropic --model claude-3-5-haiku-latest
```

`--provider` defaults to `mock` (zero config). With `--provider anthropic`, Argus calls the Anthropic Messages API (non-streaming) and records **real token usage** into the trace. More providers (OpenAI / Google / local / OpenRouter) are coming — the `Provider` trait is already model-agnostic.

### The black box (trace)

Argus writes every run to a JSONL file — one JSON object per line, one line per step. The format is open: fields include `step`, `ts_ms` (Unix milliseconds), and `kind` — a tagged object whose `type` is one of `thought` / `model_request` / `model_response` / `tool_call` / `tool_result` / `diff` / `verification_gate` / `note`, with variant-specific fields inlined alongside it. Read it with any text editor, pipe it through `jq`, or replay it with `argus trace show`.

Capability boundary: more model providers (beyond Anthropic), sandboxed tool execution, TUI, and MCP/skills import are still coming (see Roadmap).

### Time travel (fork & diff)

Re-run any recorded task with a different model or provider, then compare:

```bash
argus run "refactor this" --trace .argus/a.jsonl                       # original
argus trace fork .argus/a.jsonl --provider anthropic \
  --model claude-3-5-haiku-latest --out .argus/b.jsonl                 # re-run with a real model
argus trace diff .argus/a.jsonl .argus/b.jsonl                         # side-by-side
```

`fork` reads the original task from the trace's `task_started` event and replays it — the foundation of step-level time-travel debugging (forking from an arbitrary step lands with the multi-turn agent loop).

### Tools (multi-turn)

Argus runs a real multi-turn loop: the model can call tools, Argus executes them and feeds results back, repeating until done. Phase 3a ships file tools (sandboxed to the working directory):

- `read_file { path }` — read a UTF-8 file
- `write_file { path, content }` — write a UTF-8 file (creates parents)
- `run_shell { command }` — run a shell command in the working directory (**requires approval**)

```bash
argus run "read Cargo.toml and summarize it" --provider anthropic --model claude-3-5-haiku-latest
```

Every tool call is recorded to the trace (`tool_call` / `tool_result`).

Shell commands are gated: Argus prints each command and asks `y/N` before running. Pass `--yes` to auto-approve (use with care):

```bash
argus run "run the tests and fix failures" --provider anthropic --model claude-sonnet-4-5 --yes
```

### Verification gate (no fake "done")

Make the agent prove it's done. Pass one or more `--verify` commands; before Argus reports success, every command must exit 0 — otherwise the failure is fed back and the agent keeps fixing (up to a few attempts):

```bash
argus run "make the failing test pass" --provider anthropic --model claude-sonnet-4-5 --yes \
  --verify "cargo build" --verify "cargo test"
```

Each gate result is recorded to the trace (`verification_gate`). This is how Argus refuses to claim "done" when it isn't.

### Eval (prove it on your repo)

Quantify how reliably the agent completes tasks *on your own codebase*. Define a suite of cases — each a task plus the `verify` commands that decide pass/fail — and Argus runs them all and reports the pass-rate:

```json
{
  "name": "smoke",
  "cases": [
    { "id": "hello-endpoint", "task": "add a /hello endpoint returning 200",
      "dir": "fixtures/api", "verify": ["cargo build", "cargo test hello"] }
  ]
}
```

```bash
argus eval suite.json --provider anthropic --model claude-sonnet-4-5
```

```
eval: smoke (1 case(s))
[PASS] hello-endpoint  → .argus/eval/hello-endpoint.jsonl
1/1 passed (100%)
```

Each case writes its own trace under `--out-dir` (default `.argus/eval`), so any failure can be replayed or forked with `argus trace show`/`fork`. Argus exits non-zero if any case fails — drop it straight into CI. (MVP runs each case in place without auto-reset; keep fixtures clean between runs — results are a single-run snapshot.)

## Commands

| Command | What it does |
|---|---|
| `argus run <task> [--provider mock\|anthropic] [--model M] [--trace PATH] [--yes] [--verify CMD]` | Run a task through the agent; record every step to a JSONL trace (default `.argus/trace.jsonl`); `--yes` auto-approves shell commands; `--verify` gates completion on commands that must exit 0 |
| `argus trace show [PATH]` | Replay a recorded trace as a readable timeline |
| `argus trace fork <trace> [--provider P] [--model M] [--out PATH]` | Re-run a trace's task with a different provider/model |
| `argus trace diff <a> <b>` | Compare two traces step by step |
| `argus eval <suite.json> [--provider P] [--model M] [--out-dir DIR]` | Batch-run an eval suite; report pass-rate, write per-case traces, exit non-zero on any failure |
| `argus --version` | Print version |
| `argus --help` | Full help |

> **Coming online next:** more model providers (OpenAI / Google / local / OpenRouter) · sandboxed tool execution · TUI · MCP & skills import.

## Roadmap

- **Phase 0** — Core: agent loop · sandbox · provider abstraction · MCP · skills/AGENTS.md compat · TUI · **Trace black box**
- **Phase 1** — Reliability spearhead: **verification gate · Eval engine · time-travel debugging**
- **Phase 1.5** — **Cost-smart routing** · drift guard · circuit breaker
- **v1.0** — All four killer features, shipped.
- **Phase 2** — 24/7 multi-channel runs · lightweight governance · hosted SaaS

## License

TBD (will be OSI-approved open source).

---

<div align="center">
<sub>👁️ Argus never blinks.</sub>
</div>
