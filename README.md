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

## Roadmap

- **Phase 0** — Core: agent loop · sandbox · provider abstraction · MCP · skills/AGENTS.md compat · TUI · **Trace black box**
- **Phase 1** — Reliability spearhead: **verification gate · Eval engine · time-travel debugging**
- **Phase 1.5** — **Cost-smart routing** · drift guard · circuit breaker
- **v1.0** — All four killer features, shipped.
- **Phase 2** — 24/7 multi-channel runs · lightweight governance · hosted SaaS

See the full design in [`docs/superpowers/specs/`](docs/superpowers/specs/).

## License

TBD (will be OSI-approved open source).

---

<div align="center">
<sub>👁️ Argus never blinks.</sub>
</div>
