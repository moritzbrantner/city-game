# Prepared camera navigation

## Boundary

The browser prepares one render scene for a session's current save and viewport aspect.
Subsequent pan/zoom requests pass only the fitted overview and `RenderView` to Rust.
`three-d-camera` still produces the matrices; JavaScript does not reconstruct them.
The response contains only `camera`, and the browser retains the same node array and
node objects. An identical view returns the previous frame without a WASM call.

Preparation is disposable render data, never authoritative city state or persisted save
content. No native session registry, resource handle, cross-session cache, or new generic
renderer is introduced. The existing stateless full-frame exports remain as a reference
implementation and for callers that need an independent complete frame.

## Invalidation

Every successful command invalidates preparation except the explicit authoritative
`planning / unchanged` receipt. Rejected commands and read-only queries retain it.
An aspect change replaces the one-entry cache. Returning to an older aspect prepares
again rather than retaining an unbounded set of scenes. New scenarios have separate
sessions. Preparation/camera failures do not publish a partially updated cache.

All views are relative to the fitted overview, not the previous camera: zoom and pan
cannot accumulate projection drift. A successful mutation invalidates before the next
render, so a failed rebuild cannot resurrect geometry from the previous city state.

## Work removed

Previously every navigation frame serialized/transferred/deserialized the full save,
reprojected scenario geometry, built and sorted nodes, fitted the overview from all
corners, and serialized/transferred/deserialized every node again. That work still occurs
when preparing a changed scene, not for each camera movement. Camera-only core/transport
work and message shape are independent of entity count.

This is not a claim that the whole browser frame is constant-time: renderer validation,
scene submission, selection decoration, and picking still have their own costs. Commands
and application queries also retain the existing stateless save transport. Profile those
boundaries separately rather than interpreting this transport improvement as an FPS claim.

## Reproduce and ratchet

`cd web && bun run test:render-cache` runs dependency-free Node tests covering node identity,
10,000-node scale, duplicate views, invalidation, isolation, failures, and input cleanup.

`bash scripts/build-pages.sh` builds the canonical WASM once. The browser build then runs
those unit tests and `scripts/render-camera-smoke.mjs` against that exact WASM before
bundling. No second artifact build or separate benchmark CI lane is added.

The WASM smoke uses deterministic 1-, 64-, and 256-road scenarios and 24 camera views.
It compares camera values against the unchanged full-frame Rust path, verifies actual
planning/no-op/rejection/restart/aspect invalidation, and enforces:

- one preparation and zero legacy full-frame calls during camera-only navigation;
- retained node-array identity and no WASM call for a repeated view;
- camera request and response payloads at most 2,048 bytes per view, independent of scene size.

JSON output records preparation time, legacy/cached navigation time, and actual boundary
bytes for the same scenarios. Timings are advisory only; correctness, work counts, and
byte budgets are the gates. This benchmark excludes GPU rendering and should not be used
to report a browser FPS speedup.
