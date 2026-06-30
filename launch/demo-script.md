# Demo Script

## Goal

Create a 60-second video that makes one idea obvious:

> The agent does not get to say "done" until verification passes.

## Recording Setup

- Terminal width: 110 columns.
- Terminal height: 36 rows.
- Font size: large enough for mobile viewing.
- Theme: high contrast.
- Working directory: clean clone of Argus.
- Command to show:

```bash
argus demo
```

Optional follow-up:

```bash
argus trace show <trace-path-from-demo-output>
```

## 60-Second Script

### 0-5s: Hook

Voiceover:

```text
AI coding agents are useful now, but they still say "done" before the work is actually verified.
```

On screen:

```text
Argus: make your coding agent prove it works.
```

### 5-15s: Run Demo

Voiceover:

```text
Argus adds a verification gate around the agent loop.
```

On screen:

```bash
argus demo
```

### 15-30s: Failure Caught

Voiceover:

```text
The first result is intentionally wrong. The gate catches the failure instead of letting the agent claim success.
```

On screen:

```text
verification failed
```

or the demo output section that shows feedback being applied.

### 30-45s: Fix and Pass

Voiceover:

```text
The failure is fed back into the loop, the agent fixes the result, and only then does Argus report success.
```

On screen:

```text
result: verification passed
```

### 45-55s: Trace

Voiceover:

```text
Every step is recorded as an open trace: task, model calls, tools, and verification.
```

On screen:

```bash
argus trace show .argus/trace.jsonl
```

### 55-60s: CTA

Voiceover:

```text
Use Argus standalone, or plug it into your existing agent over MCP.
```

On screen:

```text
github.com/YOMXXX/argus
```

## Shot List

1. Hero title.
2. Install command.
3. `argus demo`.
4. Verification failure.
5. Verification pass.
6. Trace replay.
7. MCP config snippet.
8. GitHub repo CTA.

## Caption

```text
Your AI coding agent should not get to say "done" until tests pass.

Argus is an open-source black box and verification gate for coding agents.

Try it:
github.com/YOMXXX/argus
```
