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
`.argus/eval/` you can replay with `argus tui`. Fixtures are git-restored before and
after each run, so it's reproducible.

Works with any provider: swap `--provider openai --model gpt-4o-mini`, or point at a
local model with `--base-url http://localhost:11434/v1`.

## The headline number

Run the suite across models and put the pass-rate table in the top-level README — this
is the "proves it works" evidence that sets Argus apart:

| Model | Pass-rate |
|---|---|
| claude-sonnet-4-5 | _run it_ |
| gpt-4o-mini | _run it_ |
| local llama3.1 | _run it_ |

## Demo gif

`demo.tape` records the money shot (agent says done → gate catches the failing test →
agent fixes it for real) with [VHS](https://github.com/charmbracelet/vhs):

```bash
vhs benchmarks/demo.tape   # produces demo.gif
```

## Advanced: quantify the gate's value

The strongest story is *gate-on vs gate-off pass-rate*. Today `argus eval` always runs
with the gate on (the agent self-corrects until the checks pass). A future flag
(`--no-gate`) will let you run the same suite without self-correction and chart the
delta — that delta is the verification gate's measurable value.
