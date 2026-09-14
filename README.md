# city-game

`city-game` is an architecture-first city-building simulation inspired by SimCity and Cities: Skylines. Its distinguishing scenario system starts from real OpenStreetMap data: the player imports a city, takes over the resulting physical world, and develops it from there.

## Authority boundaries

- **`moenarch-geo-io-osm` / `geo-analysis` owns OSM parsing and normalization.** `city-game` consumes normalized OSM features; it does not implement PBF/XML parsing.
- **`maps` is a peer consumer of the geo/OSM foundation.** `maps` renders realistic maps; `city-game` renders a game world. Neither repository is the other's parsing layer.
- **`3d-lab` owns generic camera and browser-renderer mechanics.** `city-game` owns game-world projection, scene composition, simulation, progression, and player interaction.
- Future generic ECS, physics, assets, networking, and spatial algorithms should come from the corresponding shared repositories rather than growing local substitutes here.

The first foundation slice pins accepted revisions of both upstream foundations:

- `geo-analysis`: `c4df63a023f2183d700a9a28071d732d345e7c25`
- `3d-lab`: `6a18cb2d1fe9efdbae619c144b2180fdeb472172`

## First proven path

```text
.osm.pbf bytes
  -> moenarch-geo-io-osm
  -> deterministic CityScenario + OSM provenance
  -> city-game game-world projection
  -> three-d-camera matrices + renderer frame
  -> @moritzbrantner/three-d-renderer browser surface
```

`CitySave` keeps the imported scenario/provenance separate from mutable world state. Progression rules are data driven and evaluated deterministically until stable; a later system such as waste management can therefore depend on population, money, prior unlocks, scenario goals, or combinations of them without hard-coding the progression graph into UI code.

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
```

See [ROADMAP.md](ROADMAP.md) for the sequencing after the foundation proof and [AGENTS.md](AGENTS.md) for ownership constraints.
