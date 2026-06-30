# Argus reliability benchmark

This is how Argus *proves* it works — not by claiming, by measuring.

`reliability.json` is an [eval suite](../README.md#eval-prove-it-on-your-repo) of real
coding tasks, each with an objective pass/fail check:

| Case | Task | Passes when |
|---|---|---|
| `fix-fibonacci` | Fix a bug in `fib.py` | `python3 test_fib.py` exits 0 |
| `impl-isprime` | Implement `is_prime` (TDD) | `python3 test_is_prime.py` exits 0 |
| `edge-parse` | Handle empty/whitespace edge cases | `python3 test_parse.py` exits 0 |

## Run it (needs an API key + python3)

```bash
# from the repo root, with `argus` installed and ANTHROPIC_API_KEY set
./benchmarks/run-benchmark.sh --provider anthropic --model claude-sonnet-4-5 --yes
```

Output is a pass-rate, e.g. `3/3 passed (100%)`, plus a per-case trace under
`.argus/eval/` you can replay with `argus tui`. Each benchmark case uses
`"reset": "git"` so Argus runs it in a clean temporary worktree by default without
mutating the benchmark fixtures. Use `--in-place` only when intentionally testing
the old original-directory behavior.

Works with any provider: swap `--provider openai --model gpt-4o-mini`, or point at a
local model with `--base-url http://localhost:11434/v1`.

For stability measurements, repeat each case and write a JSON report:

```bash
./benchmarks/run-benchmark.sh --provider anthropic --model claude-sonnet-4-5 \
  --samples 5 --report-json .argus/eval/report.json
```

## The headline number

Run the suite across models and put the pass-rate table in the top-level README — this
is the "proves it works" evidence that sets Argus apart:

| Model | Pass-rate |
|---|---|
| deepseek-v4-pro | 15/15 attempts passed (100%, gate on, samples=5, 95% CI 80%-100%; 2026-06-30) |
| claude-sonnet-4-5 | _run it_ |
| gpt-4o-mini | _run it_ |
| local llama3.1 | _run it_ |

Tracked summary: [`results/deepseek-v4-pro-2026-06-30.md`](results/deepseek-v4-pro-2026-06-30.md).

## Demo gif

`demo.tape` records the money shot (agent says done → gate catches the failing test →
agent fixes it for real) with [VHS](https://github.com/charmbracelet/vhs):

```bash
vhs benchmarks/demo.tape   # produces demo.gif
```

## Advanced: quantify the gate's value

The strongest story is *gate-on vs gate-off pass-rate*. Run the same suite twice,
once with the default self-repair gate and once with `--no-gate`:

```bash
./benchmarks/run-benchmark.sh --provider anthropic --model claude-sonnet-4-5 \
  --samples 5 --report-json .argus/eval/gate-on.json
./benchmarks/run-benchmark.sh --provider anthropic --model claude-sonnet-4-5 \
  --samples 5 --no-gate --report-json .argus/eval/gate-off.json
```

`--no-gate` disables agent self-correction only; final verification still decides
pass/fail and the command still exits non-zero when any case fails.

Latest DeepSeek V4 Pro comparison:

| Run | Attempts | Cases | Wilson 95% CI |
|---|---:|---:|---:|
| Gate on | 15/15 | 3/3 | 80%-100% |
| Gate off | 14/15 | 2/3 | 70%-99% |

The one no-gate failed attempt ended with a provider response decoding error, not a
verified programming-task failure.
