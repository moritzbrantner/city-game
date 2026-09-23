import assert from "node:assert/strict";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { CityGameRuntime } from "../web/src/wasm.js";
import { harness, call } from "./city-wasm-test-harness.mjs";
import { checkCoverage, checkNavigation, summarize } from "./navigation-contract.mjs";

const root = new URL("../", import.meta.url);
const wasmPath = process.argv[2];
if (!wasmPath) throw new Error("usage: node scripts/render-camera-smoke.mjs <built.wasm>");
const { instance } = await WebAssembly.instantiate(await readFile(wasmPath), {});
const wasm = instance.exports;
const fixture = JSON.parse(await readFile(new URL("fixtures/demo-scenario.json", root)));
const budget = JSON.parse(await readFile(new URL(".performance/navigation-budget.json", root)));
assert.equal(budget.schemaVersion, 1);
const aspect = 16 / 9;
const overview = { panX: 0, panY: 0, zoom: 1 };
// Keep the PR #41 fixture and view sequence unchanged so retained byte baselines remain comparable.
const views = Array.from({ length: 24 }, (_, index) => ({
  panX: ((index % 9) - 4) / 8, panY: ((index % 7) - 3) / 8, zoom: 1 + (index % 4),
}));
const records = [];
const checks = [];

function synthetic(roadCount) {
  const scenario = structuredClone(fixture);
  scenario.roads = Array.from({ length: roadCount }, (_, index) => ({
    ...fixture.roads[0], id: `imported/way/${100 + index}`, sourceId: `way/${100 + index}`,
    geometry: {
      type: "LineString",
      coordinates: fixture.roads[0].geometry.coordinates.map(([lon, lat]) => [
        lon + (index % 32) * 0.001, lat + Math.floor(index / 32) * 0.001,
      ]),
    },
  }));
  return scenario;
}

function reference(save, view = overview, ratio = aspect) {
  // An overview returns the fitted camera directly. Re-applying an identity view
  // subtracts/re-adds large f32 frustum bounds and can introduce rounding drift.
  // Compare to the matching uncached operation, never mask discrepancies with epsilon.
  if (view.panX === 0 && view.panY === 0 && view.zoom === 1) {
    return call(wasm, "city_game_render_frame", [save], [ratio]).frame;
  }
  return call(wasm, "city_game_render_frame_view", [save, view], [ratio]).frame;
}

function structuralJourney(scenario, limits) {
  const observed = harness(wasm);
  const session = observed.runtime.createSession(scenario);
  const save = call(wasm, "city_game_new_save", [scenario]).save;
  observed.reset();
  const first = session.renderFrame(aspect);
  assert.equal(first.nodes.length, limits.nodes);
  assert.deepEqual(first, reference(save));
  const expected = views.map((view) => reference(save, view).camera);
  let last;
  for (let index = 0; index < views.length; index++) {
    last = session.renderFrame(aspect, views[index]);
    assert.strictEqual(last.nodes, first.nodes, "camera update copied scene nodes");
    assert.deepEqual(last.camera, expected[index], "camera result differs from full composition");
  }
  const result = { frames: views.length, ...observed.snapshot() };
  checkNavigation(result, { ...budget, ...limits });
  assert.strictEqual(session.renderFrame(aspect, { ...views.at(-1) }), last);
  assert.deepEqual(observed.snapshot(), (({ frames, ...stats }) => stats)(result), "duplicate view did work");
  assert.strictEqual(session.renderFrame(aspect).camera, first.camera, "overview drifted");
  assert.strictEqual(session.renderFrame(aspect).nodes, first.nodes);
  return result;
}

function timings(scenario) {
  const session = new CityGameRuntime(wasm).createSession(scenario);
  const save = call(wasm, "city_game_new_save", [scenario]).save;
  const start = performance.now();
  session.renderFrame(aspect);
  const preparationMs = performance.now() - start;
  const run = (cached) => {
    const start = performance.now();
    for (const view of views) {
      if (cached) session.renderFrame(aspect, view);
      else reference(save, view);
    }
    return performance.now() - start;
  };
  for (let index = 0; index < budget.timing.warmups; index++) { run(false); run(true); }
  const legacy = [], cached = [], ratios = [];
  for (let index = 0; index < budget.timing.samples; index++) {
    // Alternate order to avoid systematically favouring a warmer engine/CPU.
    let before, after;
    if (index % 2 === 0) { before = run(false); after = run(true); }
    else { after = run(true); before = run(false); }
    legacy.push(before); cached.push(after); ratios.push(after / Math.max(before, Number.EPSILON));
  }
  return { preparationMs, legacy: summarize(legacy), cached: summarize(cached),
    cachedOverLegacyMedian: summarize(ratios).medianMs, blocking: false };
}

for (const [id, limits] of Object.entries(budget.fixtures)) {
  const scenario = synthetic(limits.roads);
  const record = { id, nodes: limits.nodes, ...structuralJourney(scenario, limits), timings: timings(scenario) };
  records.push(record);
  console.log(JSON.stringify(record));
}
checkCoverage(records, Object.keys(budget.fixtures));

// Real, checked-in city scenarios: no network fetching or live OSM dependency during the test.
const manifest = JSON.parse(await readFile(new URL("fixtures/pages/manifest.json", root)));
assert.ok(manifest.scenarios.length >= 3);
const realCities = [];
for (const entry of manifest.scenarios) {
  const scenario = JSON.parse(await readFile(new URL(entry.scenarioPath, root)));
  const initialSave = call(wasm, "city_game_new_save", [scenario]).save;
  const nodes = reference(initialSave).nodes.length;
  assert.ok(nodes > 0, `real fixture ${entry.id} became empty`);
  const record = { id: entry.id, nodes, ...structuralJourney(scenario, {
    nodes, inputBytes: budget.maxInputBytes * views.length, outputBytes: budget.maxOutputBytes * views.length,
  }) };
  realCities.push(record);
  console.log(JSON.stringify(record));
}
checkCoverage(realCities, manifest.scenarios.map((entry) => entry.id));

// Every current planning mutation, its authoritative no-op, rejection, query, and restart.
const observed = harness(wasm);
const session = observed.runtime.createSession(fixture);
let save = call(wasm, "city_game_new_save", [fixture]).save;
const pristine = structuredClone(save);
const first = session.renderFrame(aspect);
const population = session.query({ kind: "populationState" });
const capacity = session.query({ kind: "developedPopulationCapacity" });
const planning = (command) => ({ kind: "planning", command });
const road = { id: "player/road/regression", class: "residential",
  geometry: { type: "LineString", coordinates: [[9, 49], [9.01, 49.01]] } };
const zone = (kind) => ({ id: `player/zone/${kind}`, kind,
  geometry: { type: "Polygon", coordinates: [[[9, 49], [9.01, 49], [9.01, 49.01], [9, 49]]] } });

function apply(command, expected = "applied") {
  const previous = session.renderFrame(aspect, views[0]);
  const result = session.execute(command);
  const oracle = call(wasm, "city_game_execute", [save, command]);
  save = oracle.save;
  assert.deepEqual(result, oracle.outcome);
  if (command.kind === "planning") assert.equal(result.outcome, expected);
  observed.reset();
  const next = session.renderFrame(aspect, views[0]);
  assert.deepEqual(next, reference(save, views[0]));
  assert.deepEqual(save.scenario, pristine.scenario, "planning changed immutable imported scenario");
  assert.equal(observed.snapshot().preparations, expected === "unchanged" ? 0 : 1);
  if (expected === "unchanged") assert.strictEqual(next, previous);
  else assert.notStrictEqual(next.nodes, previous.nodes);
  for (const kind of ["planning", "effectiveRoads", "populationState", "developedPopulationCapacity", "timePosition"]) {
    assert.deepEqual(session.query({ kind }), call(wasm, "city_game_query", [save, { kind }]).result);
  }
  assert.equal(observed.snapshot().liveAllocations, 0);
  assert.strictEqual(session.renderFrame(aspect, views[0]), next, "read query invalidated rendering");
  checks.push(`${command.kind}/${command.command?.kind ?? ""}/${expected}`);
  return next;
}

function reject(command) {
  const previous = session.renderFrame(aspect, views[0]);
  assert.throws(() => session.execute(command));
  assert.strictEqual(session.renderFrame(aspect, views[0]), previous, "rejection invalidated cached frame");
  assert.deepEqual(session.query({ kind: "planning" }), call(wasm, "city_game_query", [save, { kind: "planning" }]).result);
  assert.equal(observed.snapshot().liveAllocations, 0);
}

apply(planning({ kind: "addRoad", road }));
apply(planning({ kind: "addRoad", road }), "unchanged");
reject(planning({ kind: "addRoad", road: { ...road, class: "primary" } }));
reject(planning({ kind: "addRoad", road: { ...road, id: fixture.roads[0].id } }));
reject(planning({ kind: "addRoad", road: { ...road, id: " " } }));
apply(planning({ kind: "removePlayerRoad", id: road.id }));
apply(planning({ kind: "removePlayerRoad", id: road.id }), "unchanged");
for (const kind of ["residential", "commercial", "industrial", "mixedUse"]) {
  const entry = zone(kind);
  apply(planning({ kind: "zoneArea", zone: entry }));
  assert.deepEqual(session.query({ kind: "developedPopulationCapacity" }), capacity, "zoning created capacity");
  apply(planning({ kind: "zoneArea", zone: entry }), "unchanged");
  reject(planning({ kind: "addRoad", road: { ...road, id: entry.id } }));
  apply(planning({ kind: "removeZone", id: entry.id }));
  apply(planning({ kind: "removeZone", id: entry.id }), "unchanged");
}
for (const entity of [...fixture.roads, ...fixture.buildings, ...fixture.water]) {
  const suppressed = apply(planning({ kind: "suppressScenarioEntity", id: entity.id }));
  assert.ok(!suppressed.nodes.some((node) => node.id === entity.id || node.id.startsWith(entity.id + "/")));
  assert.deepEqual(session.query({ kind: "populationState" }), population, "demolition silently deleted occupants");
  if (fixture.buildings.includes(entity)) assert.notDeepEqual(session.query({ kind: "developedPopulationCapacity" }), capacity);
  apply(planning({ kind: "suppressScenarioEntity", id: entity.id }), "unchanged");
  const restored = apply(planning({ kind: "restoreScenarioEntity", id: entity.id }));
  assert.ok(restored.nodes.some((node) => node.id === entity.id || node.id.startsWith(entity.id + "/")));
  apply(planning({ kind: "restoreScenarioEntity", id: entity.id }), "unchanged");
}
reject(planning({ kind: "suppressScenarioEntity", id: "unknown/scenario/entity" }));
reject(planning({ kind: "restoreScenarioEntity", id: "unknown/scenario/entity" }));

// Fixed-seed mixed journeys catch interactions between reuse, deletion and camera/aspect changes.
let seed = 0xC17E;
const random = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed; };
for (let index = 0; index < 96; index++) {
  const add = (random() & 8) !== 0;
  const id = `player/road/mixed-${random() % 8}`;
  const exists = save.world.planning.playerRoads[id] !== undefined;
  apply(planning(add ? { kind: "addRoad", road: { ...road, id } } : { kind: "removePlayerRoad", id }),
    add === exists ? "unchanged" : "applied");
  const ratio = [0.75, 1, aspect, 3][random() % 4];
  const view = views[random() % views.length];
  assert.deepEqual(session.renderFrame(ratio, view), reference(save, view, ratio));
}
for (const view of [
  { panX: -8, panY: 8, zoom: 0.5 }, { panX: 8, panY: -8, zoom: 32 }, overview,
]) {
  for (const ratio of [0.75, 1, aspect, 3]) {
    assert.deepEqual(session.renderFrame(ratio, view), reference(save, view, ratio));
  }
}
const stable = session.renderFrame(aspect, views[0]);
for (const invalid of [
  { ...overview, zoom: 0 }, { ...overview, zoom: 32.1 }, { ...overview, panX: 8.1 },
  { ...overview, panY: -8.1 }, { ...overview, zoom: NaN }, { ...overview, panX: Infinity },
]) {
  assert.throws(() => session.renderFrame(aspect, invalid));
  assert.throws(() => session.renderFrame(0.8, invalid));
  assert.strictEqual(session.renderFrame(aspect, views[0]), stable, "failed view poisoned previous cache");
}
for (const invalid of [0, -1, Infinity, NaN]) assert.throws(() => session.renderFrame(invalid));
apply({ kind: "restart" });
assert.deepEqual(save, pristine);
assert.deepEqual(session.renderFrame(aspect), first);

// A second session must retain its own scene when the first session mutates.
const isolated = observed.runtime.createSession(synthetic(64));
const isolatedFrame = isolated.renderFrame(aspect);
apply(planning({ kind: "addRoad", road }));
assert.strictEqual(isolated.renderFrame(aspect), isolatedFrame);

// Warm allocator, then stress actual release WASM; leaks cannot hide behind a unit-test fake.
for (let index = 0; index < 96; index++) session.renderFrame(aspect, views[index % views.length]);
const warmMemoryBytes = wasm.memory.buffer.byteLength;
const retainedNodes = session.renderFrame(aspect).nodes;
for (let index = 0; index < 3_000; index++) {
  assert.strictEqual(session.renderFrame(aspect, views[index % views.length]).nodes, retainedNodes);
}
assert.equal(observed.snapshot().liveAllocations, 0);
assert.equal(wasm.memory.buffer.byteLength, warmMemoryBytes, "steady navigation grew WASM linear memory");
checks.push("3000-camera-updates/no-allocation-leaks-or-memory-growth", "session-isolation", "invalid-input-rollback");

const empty = structuredClone(fixture);
for (const key of ["roads", "buildings", "water", "landUseAreas", "transitAnchors"]) empty[key] = [];
const emptySession = new CityGameRuntime(wasm).createSession(empty);
const emptySave = call(wasm, "city_game_new_save", [empty]).save;
assert.equal(emptySession.renderFrame(aspect).nodes.length, 0);
for (const view of views) assert.deepEqual(emptySession.renderFrame(aspect, view), reference(emptySave, view));
checks.push("empty-scene");

const output = new URL(".artifacts/performance/camera-navigation.json", root);
await mkdir(new URL(".", output), { recursive: true });
const report = {
  schemaVersion: 1, suite: budget.suite,
  sourceSha: execFileSync("git", ["rev-parse", "HEAD"], { cwd: fileURLToPath(root), encoding: "utf8" }).trim(),
  baselineSource: budget.baselineSource, runtime: process.version,
  scope: "JS/WASM only; not GPU or browser FPS; timing excludes preparation and is advisory",
  records, realCities, checks, warmMemoryBytes,
};
await writeFile(output, JSON.stringify(report, null, 2) + "\n");
console.log(`Navigation regression contract passed: ${checks.length} checks; ${records.length} synthetic and ${realCities.length} real-city fixtures.`);
