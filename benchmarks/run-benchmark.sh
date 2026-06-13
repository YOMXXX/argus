#!/bin/sh
# Argus reliability benchmark — measures pass-rate on real coding tasks.
#
#   ./benchmarks/run-benchmark.sh --provider anthropic --model claude-sonnet-4-5 --yes
#
# Extra flags (--provider / --model / --yes / --base-url) are passed through to `argus eval`.
# Fixtures are restored from git before each run so results are reproducible.
set -eu

cd "$(dirname "$0")"

# Restore fixtures the agent may have modified on a previous run (reproducibility).
if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git checkout -- cases 2>/dev/null || true
fi

echo "Running Argus reliability benchmark (verification gate ON)..."
argus eval reliability.json "$@"
status=$?

# Restore fixtures again so the working tree stays clean.
if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git checkout -- cases 2>/dev/null || true
fi

exit $status
