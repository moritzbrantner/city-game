# city-game

`city-game` is an architecture-first city-building simulation inspired by SimCity and Cities: Skylines. Its distinguishing scenario system starts from real OpenStreetMap data: the player imports a city, takes over the resulting physical world, and develops it from there.

## Authority boundaries

- **`moenarch-geo-io-osm` / `geo-analysis` owns OSM parsing.** `city-game` consumes parser-level OSM features; it does not implement PBF/XML parsing.
- **`city-game` owns the game import model.** OSM is converted once into a typed, canonical `CityScenario` containing only semantics intentionally used by the simulation. Raw OSM tag maps do not become runtime state or save data.
- **`maps` is a peer consumer of the geo/OSM foundation.** `maps` renders realistic maps; `city-game` renders a game world. Neither repository is the other's parsing layer.
- **`3d-lab` owns generic camera and browser-renderer mechanics.** `city-game` owns game-world projection, scene composition, simulation, progression, planning, city time, and player interaction.
- Future generic ECS, physics, assets, networking, and spatial algorithms should come from the corresponding shared repositories rather than growing local substitutes here.

The foundation pins accepted revisions of both upstream foundations:

- `geo-analysis`: `c4df63a023f2183d700a9a28071d732d345e7c25`
- `3d-lab`: `6a18cb2d1fe9efdbae619c144b2180fdeb472172`

## Proven data path

```text
.osm.pbf bytes
  -> moenarch-geo-io-osm parser model
  -> city-game semantic selection / normalization
  -> canonical CityScenario + source provenance
  -> immutable scenario + mutable CityWorld/planning overlay
  -> CitySave + fixed-step time configuration
  -> game-world projection
  -> three-d-camera matrices + renderer frame
  -> @moritzbrantner/three-d-renderer browser surface
```

OSM is therefore an import format, not the game's persistence model. The canonical scenario currently selects explicit roads, buildings, water, land-use constraints, and transit anchors while retaining source IDs only for provenance/reimport. Player-built roads, zoning, and redevelopment decisions are stored separately from the immutable imported scenario. Repeating the same planning command is idempotent, conflicting stable IDs fail closed, and restarting a scenario resets mutable state without rewriting the import.

## Deterministic city time

Authoritative city simulation advances only through integer fixed steps. A save records how many game minutes one step represents; the default is 15 minutes and valid values must divide a 24-hour day exactly. Day and minute-of-day are derived from the tick rather than persisted separately.

Browser frame rate, elapsed wall-clock milliseconds, pause state, and playback speed are presentation/scheduling concerns only. They may determine how many fixed steps are requested, but they never change the meaning of one step. Tick overflow and invalid time configurations fail before the world is mutated.

`CitySave` stores the game-native scenario together with its fixed-step configuration and mutable world state. Progression rules are data driven and evaluated deterministically until stable; later systems such as households, economy, or waste management can therefore run against the same replayable clock without hard-coding timing into UI code.

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features

mkdir -p web/public
cargo run -p city-game-cli -- frame fixtures/demo-scenario.json web/public/demo-frame.json
cd web
bun install
bun run build
```

For a real scenario:

```sh
cargo run -p city-game-cli -- import path/to/city.osm.pbf scenario.json
cargo run -p city-game-cli -- new-save scenario.json save.json
cargo run -p city-game-cli -- step save.json save.json 96
```

The resulting `scenario.json` is the city-game format, not an OSM document. With the default 15-minute step, 96 steps advance exactly one game day.

See [ROADMAP.md](ROADMAP.md) for sequencing and [AGENTS.md](AGENTS.md) for ownership constraints.
