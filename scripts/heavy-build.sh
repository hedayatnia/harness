#!/usr/bin/env bash
# Run a heavy build or test command, one at a time across every process on
# this machine: scripts/heavy-build.sh cargo test -p harness-ui --lib
# Several agents can work in parallel worktrees; only their builds queue.

set -uo pipefail

lock="${HARNESS_HEAVY_BUILD_LOCK:-${TMPDIR:-/tmp}/harness-heavy-build.lock}"
until mkdir "$lock" 2>/dev/null; do
  # A lock whose owner has exited is stale.
  owner="$(cat "$lock/pid" 2>/dev/null || true)"
  if [[ -n "$owner" ]] && ! kill -0 "$owner" 2>/dev/null; then
    rm -rf "$lock"
    continue
  fi
  sleep 5
done
echo $$ >"$lock/pid"
trap 'rm -rf "$lock"' EXIT INT TERM
"$@"
