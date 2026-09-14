# city-game implementation contract

## Ownership

1. `geo-analysis` / `moenarch-geo-io-osm` is authoritative for reading OSM and producing parser-level OSM features. Do not add an OSM PBF/XML parser here. OSM-specific fixture creation is acceptable in tests only when it verifies the external parser boundary.
2. OSM is an **import format only**. `city-game` owns the conversion from parsed OSM features into its canonical game scenario schema. Select only semantics the simulation intentionally supports; do not carry arbitrary OSM tags into runtime state or saves.
3. `maps` and `city-game` are sibling consumers. Do not depend on `maps` merely to obtain OSM parsing. Reuse lower geo/OSM authorities instead.
4. `3d-lab` is authoritative for generic camera matrices and the reusable browser Three.js adapter. `city-game` may own a deterministic conversion from geographic scenario coordinates into game-world coordinates and may compose game-specific scene nodes, but must not grow another generic renderer or camera math layer.
5. Keep scenario normalization, simulation, progression rules, planning overlays, save state, economy, services, zoning, and city-specific rendering policy in `city-game-core`.
6. Before adding local ECS, physics, asset-pipeline, multiplayer/session, pathfinding, or generic spatial infrastructure, inspect the corresponding shared repositories and integrate through an adapter when mature enough.

## Scenario and persistence boundary

- `OSM bytes -> geo-analysis parser model -> canonical CityScenario -> mutable CityWorld/planning -> CitySave`.
- The canonical scenario is game-native. Runtime systems must not interpret OSM keys such as `highway`, `building`, or `landuse`.
- Keep per-entity source IDs only as provenance/reimport references; they are not simulation semantics.
- Imported scenario entities are immutable. Player redevelopment, removal, roads, and zoning belong in mutable game-domain state layered over the scenario.
- Saves serialize game-native scenario/world state and never serialize back to OSM.

## Determinism and provenance

- Imported source bytes are identified by SHA-256 and the exact accepted parser revision.
- Normalize canonical entity ordering before it becomes scenario state.
- Planning and progression evaluation must be deterministic, input-order independent where applicable, and idempotent.
- Save/load must preserve immutable scenario provenance exactly.
- Scenario restart resets mutable state without rewriting the canonical imported scenario.
- Rendering may derive disposable GPU/frame descriptions from effective game state, but rendering output is never simulation authority.

## Progression

Progression is rule/data driven. UI code may explain unlocks but must not decide them. Avoid arbitrary feature withholding: later systems should represent new city responsibilities and interact with systems already unlocked.

## Validation

Do not treat absent CI as green. Repository-owned validation should cover the parser-consumer boundary, absence of raw OSM tag dependence after import, deterministic planning/progression, save provenance, restart semantics, renderer-frame serialization, and the browser build. Once lockfiles are established, validation should run with frozen/locked dependency state.
