# Indexed city picking

## Cost model

Picking is an interaction-time read projection over the retained renderer scene. Scene size
is the dominant dimension: the previous implementation visited every render node and
projected all eight box corners for every click. With 20k+ render nodes that makes one
selection proportional to the whole city even when the pointer is near only a handful of
objects.

The optimized path separates one-time scene preparation from repeated picks:

- when render-node identity changes, build one disposable spatial index;
- project each pickable box once into normalized fitted-overview space using the authoritative
  3d-lab `projectWorldPoint` helper;
- bucket those immutable overview bounds in a bounded static grid;
- map a pointer plus click slop into the same overview coordinate system using city-game's
  explicit `RenderView` pan/zoom contract;
- query only overlapping buckets;
- run the exact previous screen-bounds projection and ranking algorithm on that bounded
  candidate set.

The grid is only a coarse candidate accelerator. It does not decide the selected object.

## Correctness authority

`pickNodeLinear` keeps the former full-scene algorithm as an exact differential oracle and
as a fail-closed fallback when retained scene/camera-family assumptions do not hold. Candidate
ranking remains unchanged: hit distance, projected area, entity-kind priority, center
distance, depth, then stable node id.

The index intentionally does not invert or recreate camera matrices. Generic projection
math remains owned by 3d-lab. Normalized index coordinates are derived through
`projectWorldPoint`, while conversion between overview/current normalized screen positions
uses the public city-game `RenderView` semantics.

Focused-selection bounds reuse the same per-entity index. Focusing a road therefore projects
only that road's segments instead of filtering and projecting the entire city.

## Lifetime and invalidation

The index is disposable browser projection state, never game or render authority.

- Scenario load creates one index for the accepted render-node array.
- Camera pan/zoom retains it.
- A render-frame update with a different node-array identity rebuilds it exactly once.
- The index checks retained node identity, aspect, view matrix, and the expected orthographic
  pan/zoom projection family before taking its fast path.
- If those assumptions stop holding, picking falls back to the exact linear implementation
  instead of returning a potentially stale result.

No index state is serialized into `CitySave`, `CityScenario`, or renderer output.

## Blocking work budgets

`.performance/picking-budget.json` is the structural regression policy. The synthetic
stress case contains 21,451 nodes and 256 deterministic mouse/touch queries.

After one index build, a supported pick must:

- perform zero full-scene fallback visits;
- evaluate at most 256 exact candidate bounds;
- perform at most 2,048 point projections (eight corners per candidate);
- preserve exact selected-node equality with the linear oracle in differential tests.

The build may visit each scene node once and each indexed box may project its eight corners
once. Very large boxes are retained in a small wide-item list instead of being copied into
an unbounded number of grid cells.

These are operation-count budgets, not wall-clock substitutes. Shared-runner timings remain
advisory.

## Evidence

`scripts/picking-index.test.mjs` covers:

- exact indexed/linear parity across pan, zoom, mouse/touch slop and empty space;
- the 21,451-node structural stress budget;
- camera-family fallback;
- per-entity focus bounds without a scene scan;
- scene-covering boxes without grid-reference explosion; and
- invalid-input failure behavior.

`scripts/picking-smoke.mjs` runs against the canonical release WASM and all checked-in Pages
city scenarios. It compares indexed picks with the linear oracle across several views and
pointer positions, verifies retained node identity, and emits candidate/projection work plus
advisory indexed-versus-linear timings to
`web/dist/evidence/picking.json`.

The existing Chrome/WebGL acceptance journey remains the end-to-end proof that a real canvas
click still selects the expected building and that focus/navigation remain correct. Picking
performance evidence does not replace that behavioral check.
