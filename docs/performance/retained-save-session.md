# Retained browser save session

## Ownership

The canonical browser app owns one opaque WASM session for the currently loaded city. The
session owns the live `CitySave` in Rust. JavaScript owns only the opaque handle plus disposable
render/picking projections.

Application writes still enter through `CitySave::execute(CityCommand)`; reads still use the
authoritative query boundary. The session transport does not add a second planning, persistence,
or simulation model.

The older stateless JSON ABI remains as a test/reference surface. Production browser sessions do
not use it for commands, queries, or render preparation.

## Lifetime

`CityGameRuntime.createSession` creates one Rust-owned save from canonical scenario JSON.
`CityGameSession.dispose()` destroys it deterministically and is idempotent from JavaScript.

The application disposes:

- a newly created session when an asynchronous scenario load becomes stale;
- the previous accepted session when a different scenario is committed; and
- the current session on `pagehide`.

Use after disposal is rejected in the JavaScript owner before another WASM call is made. Rust does
not keep a process-global registry of city saves; the opaque handle directly represents the owned
allocation.

## Cost model

Scenario creation is allowed to scale with canonical scenario/save size because the scenario must
cross the JS/WASM boundary once and become authoritative Rust state.

After creation:

- a command request scales with the command JSON only;
- a query request scales with the query JSON only;
- a command response contains the semantic outcome, not a copied save;
- a query response contains the requested projection only; and
- render preparation takes only the opaque handle plus aspect. It receives no serialized save.

Scene preparation can still scale with effective city geometry because it deliberately emits the
render scene. Camera-only navigation remains on the existing prepared-camera path and stays
independent of scene size after preparation.

## Regression evidence

`.performance/save-boundary-budget.json` defines the blocking structural ceilings for the named
retained-save boundary journey. The release-WASM smoke suite runs the same no-op planning command,
time-position query, and overview preparation against both a 1-road and a 4,096-road synthetic
city.

The command/query request and response byte counts must be identical across those city sizes and
remain inside their committed ceilings. This catches reintroduction of full-save serialization
without treating shared-runner wall-clock timing as a correctness signal.

The existing stateless ABI is used as a differential oracle throughout the mixed planning journey,
including applied commands, authoritative no-ops, rejected commands, restart, queries, multiple
aspects, and camera views.

The same release-WASM suite explicitly disposes independent sessions and performs a warmed
create/query/destroy loop. After warmup, repeated session lifecycle churn must not grow WASM linear
memory. Allocation-buffer ownership remains checked separately by the existing transport harness.
