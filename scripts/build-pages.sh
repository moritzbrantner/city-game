#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

python3 scripts/refresh-pages-osm.py --verify

rm -rf web/dist web/public/scenarios
mkdir -p web/public/scenarios

python3 - <<'PY'
import json
import pathlib
import re
import subprocess

manifest_path = pathlib.Path("fixtures/pages/manifest.json")
manifest = json.loads(manifest_path.read_text())
if manifest.get("schemaVersion") != 1:
    raise SystemExit("unsupported Pages scenario manifest schema")

public_scenarios = []
seen = set()
for scenario in manifest.get("scenarios", []):
    scenario_id = scenario.get("id", "")
    if not re.fullmatch(r"[a-z0-9-]+", scenario_id) or scenario_id in seen:
        raise SystemExit(f"invalid or duplicate Pages scenario id: {scenario_id!r}")
    seen.add(scenario_id)
    scenario_path = pathlib.Path(scenario["scenarioPath"])
    if not scenario_path.is_file():
        raise SystemExit(f"missing Pages scenario: {scenario_path}")

    public_scenario_path = pathlib.Path("web/public/scenarios") / f"{scenario_id}-scenario.json"
    frame_path = pathlib.Path("web/public/scenarios") / f"{scenario_id}-frame.json"
    scenario_document = json.loads(scenario_path.read_text())
    public_scenario_path.write_text(json.dumps(scenario_document, separators=(",", ":")) + "\n")
    subprocess.run(
        ["cargo", "run", "--locked", "-p", "city-game-cli", "--", "frame", str(scenario_path), str(frame_path)],
        check=True,
    )
    frame_document = json.loads(frame_path.read_text())
    frame_path.write_text(json.dumps(frame_document, separators=(",", ":")) + "\n")

    public = {key: value for key, value in scenario.items() if key != "scenarioPath"}
    public["scenario"] = f"./scenarios/{scenario_id}-scenario.json"
    public["frame"] = f"./scenarios/{scenario_id}-frame.json"
    public_scenarios.append(public)

if len(public_scenarios) < 3:
    raise SystemExit("Pages must expose at least three example city scenarios")
pathlib.Path("web/public/scenarios.json").write_text(
    json.dumps({"schemaVersion": 1, "scenarios": public_scenarios}, indent=2) + "\n"
)
PY

cargo build --locked --release -p city-game-core --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/city_game_core.wasm web/public/city-game-core.wasm

(
  cd web
  bun install --frozen-lockfile
  bun run build
)

for asset in index.html main.js surface.js style.css scenarios.json city-game-core.wasm; do
  test -s "web/dist/$asset" || {
    echo "missing Pages artifact: web/dist/$asset" >&2
    exit 1
  }
done

python3 - <<'PY'
import json
import pathlib

manifest = json.loads(pathlib.Path("web/dist/scenarios.json").read_text())
scenarios = manifest.get("scenarios", [])
if len(scenarios) < 3:
    raise SystemExit("built Pages artifact exposes fewer than three example cities")
for scenario in scenarios:
    for key in ("scenario", "frame"):
        asset = scenario.get(key, "").removeprefix("./")
        if not asset or not (pathlib.Path("web/dist") / asset).is_file():
            raise SystemExit(f"missing {key} asset for {scenario.get('id')}")
PY

grep -F 'href="./style.css"' web/dist/index.html >/dev/null
grep -F 'src="./surface.js"' web/dist/index.html >/dev/null
grep -F 'src="./main.js"' web/dist/index.html >/dev/null
grep -F 'scenarios.json' web/dist/main.js >/dev/null
grep -F 'city-game-core.wasm' web/dist/main.js >/dev/null
grep -F '@moritzbrantner/settings-browser' web/dist/index.html >/dev/null
grep -F '@moritzbrantner/input-bindings-browser' web/dist/index.html >/dev/null
grep -F 'appearance.color_scheme' web/dist/main.js >/dev/null
grep -F 'city.view.overview' web/dist/main.js >/dev/null
grep -F 'city.view.zoomIn' web/dist/main.js >/dev/null
grep -F 'Shift + drag to move' web/dist/main.js >/dev/null
grep -F 'Focus selected' web/dist/index.html >/dev/null
grep -F 'City overview' web/dist/index.html >/dev/null
grep -F 'OpenStreetMap contributors' web/dist/index.html >/dev/null
