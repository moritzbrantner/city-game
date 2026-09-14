# city-game

`city-game` is an architecture-first city-building simulation inspired by SimCity and Cities: Skylines. Its distinguishing scenario system starts from real OpenStreetMap data: the player imports a city, takes over the resulting physical world, and develops it from there.

## Authority boundaries

- **`moenarch-geo-io-osm` / `geo-analysis` owns OSM parsing.** `city-game` consumes parser-level OSM features; it does not implement PBF/XML parsing.
- **`city-game` owns the game import model.** OSM is converted once into a typed, canonical `CityScenario` containing only semantics intentionally used by the simulation. Raw OSM tag maps do not become runtime state or save data.
- **`maps` is a peer consumer of the geo/OSM foundation.** `maps` renders realistic maps; `city-game` renders a game world. Neither repository is the other's parsing layer.
- **`3d-lab` owns generic camera and browser-renderer mechanics.** `city-game` owns game-world projection, scene composition, simulation, progression, planning, city time, aggregate population/demand, and player interaction.
- Future generic ECS, physics, assets, networking, and spatial algorithms should come from the corresponding shared repositories rather than growing local substitutes here.

The foundation pins accepted revisions of both upstream foundations:

- `geo-analysis`: `c4df63a023f2183d700a9a28071d732d345e7c25`
- `3d-lab`: `6a18cb2d1fe9efdbae619c144b2180fdeb472172`

## Proven data path

```text
.osm.pbf bytes
  -> moenarch-geo-io-osm parser model
  -> city-game semantic selection / normalization
  -> canonical CityScenario + source provenance + physical game measurements
  -> immutable scenario + mutable CityWorld/planning/population state
  -> CitySave + fixed-step and population rules
  -> game-world projection
  -> three-d-camera matrices + renderer frame
  -> @moritzbrantner/three-d-renderer browser surface
```

OSM is therefore an import format, not the game's persistence model. The canonical scenario selects explicit roads, buildings, water, land-use constraints, and transit anchors while retaining source IDs only for provenance/reimport. Physical measurements needed by later systems are also materialized at that boundary: imported buildings carry integer `grossFloorAreaM2`, derived once from source footprint plus level/height information. Runtime population logic does not need to interpret OSM tags or recompute geographic floor area.

Player-built roads, zoning, and redevelopment decisions are stored separately from the immutable imported scenario. Repeating the same planning command is idempotent, conflicting stable IDs fail closed, and restarting a scenario resets mutable state without rewriting the import.

## Deterministic city time

Authoritative city simulation advances only through integer fixed steps. A save records how many game minutes one step represents; the default is 15 minutes and valid values must divide a 24-hour day exactly. Day and minute-of-day are derived from the tick rather than persisted separately.

Browser frame rate, elapsed wall-clock milliseconds, pause state, and playback speed are presentation/scheduling concerns only. They may determine how many fixed steps are requested, but they never change the meaning of one step. Tick overflow and invalid time configurations fail before the world is mutated.

## Aggregate population and RCI demand

The playable-city foundation models households and jobs as aggregate stocks rather than creating one entity per citizen. Residential capacity is derived from residential building floor area; commercial and industrial job capacity are derived independently from their developed floor area. Explicit `PopulationRules` define the floor-area-per-unit assumptions plus initial and target occupancy.

A new save seeds aggregate occupied households/jobs deterministically from the immutable imported building stock. If a player suppresses a building for redevelopment, developed capacity falls but existing occupants are not silently erased; the difference appears as bounded signed residential/commercial/industrial demand pressure. This makes redevelopment and future construction interact with the simulation without requiring individual citizen agents yet.

Zoning is intentionally not capacity. A residential/commercial/industrial zone marks where later property development may occur, but no household or job capacity exists there until the development system creates an actual game building. That keeps zoning, construction, occupancy, and demand as separate concepts for the next economy/land slices.

`CitySave` stores the game-native scenario together with fixed-step configuration, population rules, and mutable world state. Save/resume preserves these aggregates exactly, while scenario restart reseeds the original scenario-derived population baseline.

## GitHub Pages

The browser version is a required delivery surface for `city-game`. The canonical project-site URL is:

`https://moritzbrantner.github.io/city-game/`

`bash scripts/build-pages.sh` is the repository-owned build contract used by both validation and Pages delivery. It regenerates the browser renderer frame through `city-game-cli`, installs the locked Bun dependencies, builds `web/dist`, and verifies the relative assets required for project-site hosting.

Pushes to `main` are delivered through the shared `reusable-workflows` build-artifact and Pages workflows. The deploy job consumes the exact verified artifact produced for the source commit rather than rebuilding it during deployment. If GitHub Pages has not yet been enabled for the repository, the Pages preflight reports the one-time repository setting instead of treating an absent deployment as green.

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
bash scripts/build-pages.sh
```

For a real scenario:

```sh
cargo run -p city-game-cli -- import path/to/city.osm.pbf scenario.json
cargo run -p city-game-cli -- new-save scenario.json save.json
cargo run -p city-game-cli -- step save.json save.json 96
```

The resulting `scenario.json` is the city-game format, not an OSM document. With the default 15-minute step, 96 steps advance exactly one game day.

See [ROADMAP.md](ROADMAP.md) for sequencing and [AGENTS.md](AGENTS.md) for ownership constraints.
