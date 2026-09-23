#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

cargo build --locked --release -p city-game-cli >/dev/null
binary="$root/target/release/city-game-cli"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

"$binary" new-save fixtures/demo-scenario.json "$tmp/initial-save.json"

measure_step() {
  local output="$1"
  local start end
  start="$(date +%s%N)"
  "$binary" step "$tmp/initial-save.json" "$output" 10000
  end="$(date +%s%N)"
  printf '%s' "$((end - start))"
}

first_elapsed="$(measure_step "$tmp/step-first.json")"
second_elapsed="$(measure_step "$tmp/step-second.json")"
cmp --silent "$tmp/step-first.json" "$tmp/step-second.json" || {
  echo "city-game fixed-step performance journey became nondeterministic" >&2
  diff -u "$tmp/step-first.json" "$tmp/step-second.json" >&2 || true
  exit 1
}

frame_start="$(date +%s%N)"
"$binary" frame fixtures/demo-scenario.json "$tmp/frame-first.json"
frame_end="$(date +%s%N)"
"$binary" frame fixtures/demo-scenario.json "$tmp/frame-second.json"
cmp --silent "$tmp/frame-first.json" "$tmp/frame-second.json" || {
  echo "city-game render-frame composition became nondeterministic" >&2
  diff -u "$tmp/frame-first.json" "$tmp/frame-second.json" >&2 || true
  exit 1
}

save_bytes="$(wc -c < "$tmp/step-first.json" | tr -d ' ')"
frame_bytes="$(wc -c < "$tmp/frame-first.json" | tr -d ' ')"
printf '{"scenario":"fixed-step-10000","firstElapsedNs":%s,"secondElapsedNs":%s,"saveBytes":%s,"deterministic":true,"timing":"advisory-hosted-linux"}\n' \
  "$first_elapsed" "$second_elapsed" "$save_bytes"
printf '{"scenario":"render-frame-demo","elapsedNs":%s,"frameBytes":%s,"deterministic":true,"timing":"advisory-hosted-linux"}\n' \
  "$((frame_end - frame_start))" "$frame_bytes"

# Timings remain advisory. Deterministic unit tests enforce the blocking work-count ratchets:
# one clock write per non-zero batch and zero immutable-building visits during advancement.
cargo bench --locked --quiet -p city-game-core --bench fixed_step -- --smoke
