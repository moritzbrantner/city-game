# city-game implementation contract

## Ownership

1. `geo-analysis` / `moenarch-geo-io-osm` is authoritative for reading and normalizing OSM. Do not add an OSM PBF/XML parser here. OSM-specific fixture creation is acceptable in tests only when it verifies the external parser boundary.
2. `maps` and `city-game` are sibling consumers. Do not depend on `maps` merely to obtain OSM parsing. Reuse lower geo/OSM authorities instead.
3. `3d-lab` is authoritative for generic camera matrices and the reusable browser Three.js adapter. `city-game` may own a deterministic conversion from geographic scenario coordinates into game-world coordinates and may compose game-specific scene nodes, but must not grow another generic renderer or camera math layer.
4. Keep simulation, progression rules, scenario semantics, save state, economy, services, zoning, and city-specific rendering policy in `city-game-core`.
5. Before adding local ECS, physics, asset-pipeline, multiplayer/session, pathfinding, or generic spatial infrastructure, inspect the corresponding shared repositories and integrate through an adapter when mature enough.

## Determinism and provenance

- Imported source bytes are identified by SHA-256 and the exact accepted parser revision.
- Normalize feature ordering before it becomes scenario state.
- Progression evaluation must be deterministic, input-order independent, and idempotent.
- Save/load must preserve immutable scenario provenance exactly.
- Rendering may derive disposable GPU/frame descriptions from scenario state, but rendering output is never simulation authority.

## Progression

Progression is rule/data driven. UI code may explain unlocks but must not decide them. Avoid arbitrary feature withholding: later systems should represent new city responsibilities and interact with systems already unlocked.

## Validation

Do not treat absent CI as green. Repository-owned validation should cover the parser-consumer boundary, deterministic progression, save provenance, renderer-frame serialization, and the browser build. Once lockfiles are established, validation should run with frozen/locked dependency state.
