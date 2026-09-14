# city-game roadmap

## 0. Foundation proof — complete

- Consume `moenarch-geo-io-osm` for in-memory `.osm.pbf` parsing.
- Transform parsed OSM objects into a stable, game-native `CityScenario`; OSM tags stop at the import boundary.
- Preserve source SHA-256, exact upstream parser revision, and per-entity source references for provenance/reimport.
- Materialize simulation-relevant physical measurements such as building gross floor area into canonical scenario data at import time.
- Keep mutable `CityWorld` / `CitySave` state separate from immutable imported scenario state.
- Establish deterministic, data-driven progression rules.
- Project scenario coordinates into a game-local world and emit the reusable `3d-lab` renderer contract using `three-d-camera` for camera matrices.
- Build a browser proof with `@moritzbrantner/three-d-renderer` rather than a local Three.js renderer.
- Publish the canonical browser surface through GitHub Pages from an exact-source, reusable-workflows-qualified artifact rather than a separate demo implementation.

## 1. Playable city skeleton — current

- Treat canonical imported roads/building footprints/water/land use as the initial scenario, not a background image or runtime OSM model.
- Add player-owned road and zoning changes as a deterministic overlay on immutable scenario state; redevelopment suppresses/replaces scenario entities without rewriting the import.
- Save/resume and scenario restart operate on the game-native scenario/world format, never OSM.
- Use deterministic fixed-step city time: a save owns the validated step scale, the world stores an integer tick, and calendar position is derived without wall-clock simulation authority.
- Add aggregate households and commercial/industrial jobs from canonical developed floor area; derive bounded R/C/I pressure without per-citizen ECS state.
- Keep zoning as eligibility for future development rather than immediate population/job capacity.
- **Next:** add interactive planning controls over the proven road/zoning command boundary, then connect zoning demand to property development in the economy/land phase.

## 2. Economy and land

- Budget, taxes, construction/maintenance cost, demand, land value, and property development.
- Turn zoned eligible land plus R/C/I demand into deterministic property-development decisions and canonical game buildings rather than UI-only growth.
- Make real-city constraints matter: existing road topology, water, terrain/land use, and demolition/redevelopment costs.
- Add scenario goals independently of progression unlock definitions.

## 3. First responsibility unlocks

- Introduce utilities and service responsibilities through data-driven rules.
- Add waste/trash management as a genuine new responsibility after an appropriate goal is reached; exact balance thresholds belong in scenario/rules data rather than UI constants.
- Make unlocked systems interact with growth, budget, health, attractiveness, and land value.

## 4. Mature city systems

- Education, fire/police/health, power/water, pollution, logistics, public transport, traffic pressure, industry chains, parks/leisure, and environmental constraints.
- Reuse shared pathfinding/spatial/ECS foundations where their authority boundaries fit; do not create generic substitutes locally.

## 5. Scenario depth

- Real-city scenario objectives and constraints derived from imported geography.
- Multiple progression profiles: sandbox, historical/redevelopment, growth, fiscal, mobility, and environmental challenges.
- Deterministic scenario/version migrations so updated game rules do not silently reinterpret old saves.

## 6. Scale and fidelity

- Profile large OSM extracts before choosing citizen/vehicle simulation granularity.
- Add LOD/streaming only from measured pressure.
- Route reproducible models/materials/props through `asset-tooling` and generic rendering primitives through `3d-lab`.
