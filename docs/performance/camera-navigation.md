# Prepared camera navigation and regression ratchets

## Ownership and reuse

The browser prepares one disposable scene per session/save/aspect. Camera changes pass
only the fitted overview and `RenderView` to Rust; `three-d-camera` still produces all
matrices. Nodes retain their array/object identity. An identical view crosses no WASM
boundary. Preparation is not saved, authoritative state, or a cross-session registry.

Every successful command invalidates preparation except the explicit authoritative
`planning / unchanged` receipt. Rejected commands and read-only queries retain it.
Aspect changes replace the bounded one-entry cache. Failed preparation/camera operations
cannot publish partial data. Every view is relative to the fitted overview, not the last
camera, avoiding accumulated drift.

The browser presentation layer follows the same retained identity. When the runtime returns
the same node array, only the camera contract is revalidated. `CityFramePresenter` keeps
the last submitted scene and calls the shared 3d-lab `renderCamera` fast path, which draws
the retained Three.js scene without scene-node/resource reconciliation. Full validation and
full `render(frame)` remain mandatory whenever the node array changes. Selection decoration
is materialized once per `(node array, selected entity, accent)` and reused while the camera
moves; a selection, accent, or scene change submits a fresh full scene.

## Blocking contract

`.performance/navigation-budget.json` records the accepted PR #41 source and run. The
1-, 64-, and 256-road fixtures retain their measured camera input/output byte ceilings,
not a freshly measured baseline from the candidate under test. The new 4,096-road fixture
(8,194 render nodes) and checked-in real-city fixtures share size-independent per-view
limits: 352 request bytes and 512 response bytes, tightened from 2,048 each.

The release-WASM suite fails on extra scene preparations, legacy full-frame calls,
unexpected save/query/command traffic during navigation, extra camera calls, increased
payloads, missing benchmark fixtures, changed camera results, copied node arrays, leaked
input/output allocations, or steady-state WASM memory growth. Native correctness tests
continue to run independently in Validate. Neither timing nor a faster wrong answer can
replace correctness evidence.

The Node tests use guarded node arrays at both retention boundaries. One cache test rejects
any traversal during 10,000 WASM camera changes. A presentation test first submits a
21,451-node scene, then requires 10,000 camera frames to perform zero additional node reads
and zero full renderer submissions. The same test covers selected-scene decoration reuse,
selection/accent changes, renderer replacement, and changed-node fallback to full validation.

Actual production-cache mutation tests intentionally reintroduce six faults:
per-view rebuilding, copied node arrays, duplicate-view work, missing mutation invalidation,
missing aspect invalidation, and retaining a caller-owned mutable view object. The oracle
must accept the original implementation and reject each syntactically valid mutant.
Synthetic broken metric samples exercise every performance assertion, including a one-byte
increase. Syntax/import errors do not count as detected runtime regressions.

## Correctness scenarios

The real release WASM is compared with the uncached full-save/full-frame reference. Tests
cover all six planning commands and four zone kinds, repeated no-ops, conflicting/reserved
IDs, deletion and restoration, retained population during suppression, zoning not creating
capacity, immutable scenario provenance, restart, empty scenes, separate sessions, invalid
camera requests, min/max pan and zoom, four aspects, and a 96-command fixed-seed mixed
journey. After warming the allocator, 3,000 additional camera changes must neither leak
allocations nor grow WASM linear memory. All checked-in Pages city fixtures are exercised
without requesting new OSM data.

## Browser coverage

`scripts/browser-navigation-smoke.mjs` runs installed Chrome/Chromium via native Node CDP
against the built production JS/WASM/WebGL app under `/city-game/`. A deterministic test
manifest supplies the existing demo geometry; it does not replace the renderer or core.
The test exercises zoom/overview, unchanged-view no-op, building selection, focus,
unmodified drag, cancelled/foreign pointers, coalesced Shift-drag, CSS resize, and scenario
switch reset. Camera-only movement must not rebuild scenes or upload geometry to WebGL.
The displayed GPU projection must still match Rust for zoom and pan. Structural Node tests
cover the CPU-side zero-node-visit requirement that buffer-upload checks alone cannot prove.

Optional external settings/input distributions are deliberately blocked in this offline
lane. This proves core buttons/pointer interactions and graceful optional-module absence,
not shared keyboard/wheel integration. Missing Chrome or Node 22+ fails explicitly instead
of silently skipping. `CITY_GAME_CHROME` can name an installed browser executable.

## Timing evidence and limitations

Correctness/work-count runs and timings are separate. Timings use two warmups followed by
seven alternating-order legacy/cached pairs, reporting median, p95, minimum, maximum and
sample count. Preparation is measured separately. These are advisory local JS/WASM timings,
not network traffic or browser FPS. With a retained scene, the city-game presentation layer
and shared renderer camera submission are now constant in scene-node count; the Three.js/WebGL
draw itself can still scale with visible GPU work. A regression that changes cost without
changing the structural counters may still require
review of timing evidence or a controlled-machine profiler. No finite test suite guarantees
all future correctness or the absence of every possible slowdown.

## Running and retaining evidence

- `cd web && bun run test:render-cache`: dependency-free Node guards and mutation tests.
- `bash scripts/build-pages.sh`: canonical release WASM and browser artifact, then all
  navigation checks against that exact build. No extra artifact build or CI job is added.
- `cd web && bun run test:render-wasm`: repeat the release-WASM suite using the existing
  `public/city-game-core.wasm`.
- `cd web && bun run test:browser`: repeat browser checks on the existing `dist` artifact.

`web/dist/evidence/camera-navigation.json` includes source SHA, retained baseline SHA,
fixture results, repeated timing distributions, and correctness journeys.
`browser-navigation.json` records browser version, scope, checks, and boundary counts.
`navigation.png` shows the actual selected-building browser fixture. These files travel
with the existing CI build artifact, not a separate unverified rebuild.

Baseline changes are explicit reviewed source changes: explain the intentional feature or
representation change and retain matching correctness evidence. Do not regenerate or relax
budgets merely to make a failing candidate green.
