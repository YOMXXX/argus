# DeepSeek V4 Pro Reliability Benchmark - 2026-06-30

Suite: `argus-reliability-bench`

Provider: `openai`

Base URL: `https://api.deepseek.com`

Model: `deepseek-v4-pro`

## Summary

| Run | Gate | Samples | Case pass-rate | Attempt pass-rate | Wilson 95% CI |
|---|---:|---:|---:|---:|---:|
| Gate on | on | 5 | 3/3 | 15/15 (100%) | 80%-100% |
| Gate off | off | 5 | 2/3 | 14/15 (93%) | 70%-99% |

## Case Breakdown

| Case | Gate on | Gate off |
|---|---:|---:|
| `fix-fibonacci` | 5/5 | 5/5 |
| `impl-isprime` | 5/5 | 4/5 |
| `edge-parse` | 5/5 | 5/5 |

The single no-gate failed attempt ended with a provider response decoding error, so this
result should be treated as a reliability signal rather than a claim that the model
failed the programming task.

Local ignored reports:

- `.argus/eval/deepseek-v4-pro-gate-on-s5.json`
- `.argus/eval/deepseek-v4-pro-no-gate-s5.json`
