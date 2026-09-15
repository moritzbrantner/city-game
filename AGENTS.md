# city-game implementation contract

## Ownership

1. `geo-analysis` / `moenarch-geo-io-osm` is authoritative for reading OSM and producing parser-level OSM features. Do not add an OSM PBF/XML parser here. OSM-specific fixture creation is acceptable in tests only when it verifies the external parser boundary.
2. OSM is an **import format only**. `city-game` owns the conversion from parsed OSM features into its canonical game scenario schema. Select only semantics the simulation intentionally supports; do not carry arbitrary OSM tags into runtime state or saves.
3. `maps` and `city-game` are sibling consumers. Do not depend on `maps` merely to obtain OSM parsing. Reuse lower geo/OSM authorities instead.
4. `3d-lab` is authoritative for generic camera matrices and the reusable browser Three.js adapter. `city-game` may own a deterministic conversion from geographic scenario coordinates into game-world coordinates and may compose game-specific scene nodes, but must not grow another generic renderer or camera math layer.
5. Keep scenario normalization, simulation, aggregate population/jobs, progression rules, planning overlays, city time, save state, economy, services, zoning, and city-specific rendering policy in `city-game-core`.
6. Before adding local ECS, physics, asset-pipeline, multiplayer/session, pathfinding, or generic spatial infrastructure, inspect the corresponding shared repositories and integrate through an adapter when mature enough.

## Application and simulation boundary

- Use lightweight CQS/CQRS for application and player intent, not as the internal architecture of the simulation engine.
- `CityCommand` represents semantic player/application mutations such as planning changes or restarting a scenario. Do not route fixed-step advancement, progression evaluation, economy ticks, service updates, AI, pathfinding, ECS systems, physics, or other simulation mechanics through the command gateway.
- Simulation systems are direct deterministic `city-game-core` operations coordinated by authoritative fixed-step simulation. Their behavior must remain equivalent for batched versus repeated single steps where applicable.
- Queries may provide read-oriented shapes over authoritative state, but do not add asynchronous projections, a separate read store, messaging, or event sourcing without a concrete requirement that justifies their consistency and operational cost.
- Keep command handlers thin: validate application intent and delegate domain rules to their authoritative core systems rather than duplicating business logic in transport/UI layers.

## CQRS and rules boundary

- Application/player mutations enter through `CitySave::execute(CityCommand)`. Do not expose parallel public mutation helpers or public mutable save/world fields that allow callers to bypass command validation. Internal deterministic simulation operations are not application commands.
- Application reads go through `CitySave::queries()` or other explicitly read-only projections. Query code must not mutate authoritative game state.
- CQRS is an in-process domain boundary. Do not add an event store, replay log, or event-sourcing infrastructure unless a later requirement explicitly justifies it.
- `CityWorld` is authoritative mutable simulation state. Persistence stores current state rather than reconstructing it from events.
- `CityRuleset` is authoritative for subsystem enablement and rule configuration. Keep each subsystem's status and typed configuration together in its rule module; do not scatter browser/UI feature flags or generic key/value rule maps through the codebase.
- Disabled rule modules are inert. Their commands/queries are unavailable, and dormant configuration must not block unrelated systems. When a module is enabled, its typed configuration must validate before authoritative mutation.
- Prefer typed `PopulationRules`, `ProgressionRules`, future `EconomyRules`, `WasteRules`, `TrafficRules`, etc. over a generic expression-language rules engine.

## Scenario and persistence boundary

- `OSM bytes -> geo-analysis parser model -> canonical CityScenario -> mutable CityWorld/planning -> CitySave`.
- The canonical scenario is game-native. Runtime systems must not interpret OSM keys such as `highway`, `building`, or `landuse`.
- Keep per-entity source IDs only as provenance/reimport references; they are not simulation semantics.
- Materialize physical measurements that later simulation needs at import time when they are derived from geographic source geometry. Building `grossFloorAreaM2` is canonical game data after import; population simulation must not repeatedly reinterpret latitude/longitude geometry or OSM level tags.
- Imported scenario entities are immutable. Player redevelopment, removal, roads, and zoning belong in mutable game-domain state layered over the scenario.
- Saves serialize game-native scenario/world state and never serialize back to OSM.
- For now, scenario, save, and ruleset persistence accept only the exact current schema versions. Do not add implicit legacy defaults or compatibility migrations until persisted backward compatibility becomes an explicit product requirement.

## Aggregate population and demand

- Household and job simulation is aggregate until the roadmap has a demonstrated need for individual agents. Do not introduce per-citizen ECS state merely to calculate zoning demand.
- Developed residential/commercial/industrial capacity comes from effective canonical building stock and integer gross floor area using the population module's explicit `PopulationRules`.
- Aggregate occupied households/jobs are mutable city state. Suppressing or redeveloping a building changes capacity but must not silently delete occupants.
- R/C/I demand is a bounded signed integer pressure derived from occupied stock versus developed capacity and target occupancy.
- A zone is development eligibility, not occupied capacity. Zoning polygons do not create households/jobs or building capacity until the property-development system produces actual game buildings.
- Save/resume preserves the typed ruleset and aggregate stocks exactly in the current schema. Scenario restart reseeds aggregate stocks from the immutable canonical scenario when population rules are enabled before replacing mutable world state.

## Determinism, time, and provenance

- Imported source bytes are identified by SHA-256 and the exact accepted parser revision.
- Normalize canonical entity ordering before it becomes scenario state.
- Planning and progression evaluation must be deterministic, input-order independent where applicable, and idempotent.
- Authoritative simulation advances only in explicit integer fixed steps. Browser frame delta, wall-clock duration, and playback speed are not simulation inputs.
- A save owns the fixed-step time configuration; `CityWorld` stores the tick only. Calendar position is derived rather than duplicated as mutable elapsed-time state.
- Invalid enabled rule configuration, invalid time configuration, and tick/capacity overflow must fail before partial world mutation.
- Batch stepping must be equivalent to repeated single stepping.
- Save/load must preserve immutable scenario provenance, fixed-step configuration, typed ruleset, and aggregate population state exactly for the current schema.
- Scenario restart resets mutable state without rewriting the canonical imported scenario or changing save-level configuration.
- Rendering may derive disposable GPU/frame descriptions from effective game state, but rendering output is never simulation authority.

## Progression

Progression is rule/data driven. UI code may explain unlocks but must not decide them. Avoid arbitrary feature withholding: later systems should represent new city responsibilities and interact with systems already unlocked.

## Browser delivery

- A GitHub Pages version is a required `city-game` delivery surface, not an optional documentation demo.
- `web/` is the canonical browser application. Do not create a parallel Pages-only implementation or duplicate simulation rules in browser code.
- Browser player intent that changes game state must cross the same `CityCommand` boundary used by other application adapters; do not reimplement planning/rule decisions in JavaScript. Internal simulation stepping remains a Rust simulation operation rather than a synthetic command.
- Build the Pages artifact through `scripts/build-pages.sh`; it must regenerate game-owned renderer data through the canonical Rust CLI and use frozen browser dependencies.
- Keep project-site assets relative so the same artifact works at `/city-game/` without repository-name logic in simulation or rendering code.
- Deployment must consume an exact-source, verified build artifact. Do not silently rebuild a different artifact in the deploy job.
- If Pages is not configured, validation/deployment must report that state explicitly rather than treating missing publication as success.

## Validation

Do not treat absent CI as green. Repository-owned validation should cover the parser-consumer boundary, absence of raw OSM tag dependence after import, canonical physical measurements, deterministic command/query/rule behavior, planning/progression/time/population demand, strict current-schema persistence, save provenance, restart semantics, renderer-frame serialization, the canonical browser build, and the required Pages delivery contract. Once lockfiles are established, validation should run with frozen/locked dependency state.
