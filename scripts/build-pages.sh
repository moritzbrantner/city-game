#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

rm -rf web/dist
mkdir -p web/public
rm -f web/public/demo-frame.json

cargo run --locked -p city-game-cli -- frame fixtures/demo-scenario.json web/public/demo-frame.json

(
  cd web
  bun install --frozen-lockfile
  bun run build
)

for asset in index.html main.js style.css demo-frame.json; do
  test -s "web/dist/$asset" || {
    echo "missing Pages artifact: web/dist/$asset" >&2
    exit 1
  }
done

grep -F 'href="./style.css"' web/dist/index.html >/dev/null
grep -F 'src="./main.js"' web/dist/index.html >/dev/null
grep -F 'demo-frame.json' web/dist/main.js >/dev/null
