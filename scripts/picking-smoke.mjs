import assert from "node:assert/strict";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { CityGameRuntime } from "../web/src/wasm.js";
import {
  CityPickingIndex,
  entityScreenBoundsLinear,
  pickNodeLinear,
} from "../web/src/picking-index.js";

const root = new URL("../", import.meta.url);
const wasmPath = process.argv[2];
if (!wasmPath) throw new Error("usage: node scripts/picking-smoke.mjs <built.wasm>");

const { instance } = await WebAssembly.instantiate(await readFile(wasmPath), {});
const wasm = instance.exports;
const runtime = new CityGameRuntime(wasm);
const budget = JSON.parse(await readFile(new URL(".performance/picking-budget.json", root)));
const manifest = JSON.parse(await readFile(new URL("fixtures/pages/manifest.json", root)));
const aspect = 16 / 9;
const views = [
  { panX: 0, panY: 0, zoom: 1 },
  { panX: 0.25, panY: -0.125, zoom: 2 },
  { panX: -0.5, panY: 0.5, zoom: 0.5 },
];
const pointers = [
  { x: 120, y: 90, width: 1280, height: 720, pointerType: "mouse" },
  { x: 360, y: 180, width: 1280, height: 720, pointerType: "mouse" },
  { x: 640, y: 360, width: 1280, height: 720, pointerType: "mouse" },
  { x: 920, y: 240, width: 1280, height: 720, pointerType: "touch" },
  { x: 1160, y: 600, width: 1280, height: 720, pointerType: "mouse" },
  { x: 80, y: 650, width: 1280, height: 720, pointerType: "touch" },
  { x: 760, y: 620, width: 1280, height: 720, pointerType: "mouse" },
  { x: 1100, y: 110, width: 1280, height: 720, pointerType: "mouse" },
];
const records = [];

function assertScreenBoundsEquivalent(actual, expected) {
  assert.equal(actual === null, expected === null);
  if (!actual || !expected) return;
  for (const key of ["minX", "maxX", "minY", "maxY"]) {
    const tolerancePx = 1e-3;
    assert.ok(
      Math.abs(actual[key] - expected[key]) <= tolerancePx,
      `${key} diverged by more than ${tolerancePx}px: ${actual[key]} vs ${expected[key]}`,
    );
  }
}

function entityKinds(scenario) {
  const kinds = new Map();
  for (const entity of scenario.roads ?? []) kinds.set(entity.id, "Road");
  for (const entity of scenario.buildings ?? []) kinds.set(entity.id, "Building");
  for (const entity of scenario.water ?? []) kinds.set(entity.id, "Water");
  for (const entity of scenario.landUseAreas ?? []) kinds.set(entity.id, "Land use");
  return kinds;
}

for (const entry of manifest.scenarios) {
  const scenario = JSON.parse(await readFile(new URL(entry.scenarioPath, root)));
  const kinds = entityKinds(scenario);
  const kindForEntity = (id) => kinds.get(id);
  const session = runtime.createSession(scenario);
  const overview = session.renderFrame(aspect, views[0]);

  const buildStart = performance.now();
  const index = new CityPickingIndex(overview, views[0]);
  const buildMs = performance.now() - buildStart;
  assert.equal(index.buildObservations.nodeVisits, overview.nodes.length);
  assert.equal(
    index.buildObservations.projectionCalls,
    index.buildObservations.indexedNodes * budget.buildProjectionCallsPerIndexedBox,
  );

  let indexedMs = 0;
  let linearMs = 0;
  let maxCandidateBoundsEvaluations = 0;
  let maxProjectionCalls = 0;
  let totalCandidateBoundsEvaluations = 0;
  let queryCount = 0;

  for (const view of views) {
    const frame = session.renderFrame(aspect, view);
    assert.strictEqual(frame.nodes, overview.nodes, "camera navigation must retain picking scene identity");
    for (const pointer of pointers) {
      const indexedStart = performance.now();
      const fast = index.pick(frame, view, pointer, kindForEntity);
      indexedMs += performance.now() - indexedStart;

      const linearStart = performance.now();
      const reference = pickNodeLinear(frame, pointer, kindForEntity);
      linearMs += performance.now() - linearStart;

      assert.equal(
        fast.node?.id ?? null,
        reference.node?.id ?? null,
        `${entry.id} indexed pick diverged at ${JSON.stringify({ view, pointer })}`,
      );
      assert.equal(fast.observations.path, "indexed");
      assert.equal(
        fast.observations.fullSceneNodeVisits,
        budget.maxFullSceneFallbackNodeVisits,
      );
      assert.ok(
        fast.observations.candidateBoundsEvaluations <=
          budget.maxCandidateBoundsEvaluationsPerPick,
        `${entry.id} absolute candidate budget: ${JSON.stringify(fast.observations)}`,
      );
      assert.ok(
        fast.observations.candidateBoundsEvaluations / frame.nodes.length <=
          budget.maxCandidateFractionPerPick,
        `${entry.id} relative candidate budget: ${JSON.stringify(fast.observations)}`,
      );
      assert.ok(
        fast.observations.projectionCalls <= budget.maxCandidateProjectionCallsPerPick,
        `${entry.id} projection budget: ${JSON.stringify(fast.observations)}`,
      );

      maxCandidateBoundsEvaluations = Math.max(
        maxCandidateBoundsEvaluations,
        fast.observations.candidateBoundsEvaluations,
      );
      maxProjectionCalls = Math.max(maxProjectionCalls, fast.observations.projectionCalls);
      totalCandidateBoundsEvaluations += fast.observations.candidateBoundsEvaluations;
      queryCount++;
    }
  }

  const focusEntity = scenario.buildings?.[0]?.id ?? scenario.roads?.[0]?.id;
  if (focusEntity) {
    const view = views[1];
    const frame = session.renderFrame(aspect, view);
    const fastBounds = index.screenBoundsForEntity(frame, view, focusEntity, 1280, 720);
    const referenceBounds = entityScreenBoundsLinear(frame, focusEntity, 1280, 720);
    assertScreenBoundsEquivalent(fastBounds.bounds, referenceBounds.bounds);
    assert.ok(
      fastBounds.observations.nodeVisits < referenceBounds.observations.nodeVisits ||
        frame.nodes.length <= fastBounds.observations.nodeVisits,
      "focused-entity lookup should avoid a full scene scan",
    );
  }

  const record = {
    id: entry.id,
    nodes: overview.nodes.length,
    queries: queryCount,
    build: index.buildObservations,
    maxCandidateBoundsEvaluations,
    averageCandidateBoundsEvaluations:
      totalCandidateBoundsEvaluations / Math.max(1, queryCount),
    maxProjectionCallsPerPick: maxProjectionCalls,
    indexedMs,
    linearReferenceMs: linearMs,
    buildMs,
    timing: "advisory-no-wall-clock-gate",
  };
  records.push(record);
  console.log(JSON.stringify(record));
  session.dispose();
}

const output = new URL(".artifacts/performance/picking.json", root);
await mkdir(new URL(".", output), { recursive: true });
await writeFile(
  output,
  JSON.stringify(
    {
      schemaVersion: 1,
      suite: budget.suite,
      sourceSha: execFileSync("git", ["rev-parse", "HEAD"], {
        cwd: fileURLToPath(root),
        encoding: "utf8",
      }).trim(),
      runtime: process.version,
      scope:
        "Release WASM city frames plus browser picking projection math; structural budgets block, timings are advisory.",
      records,
    },
    null,
    2,
  ) + "\n",
);
console.log(`Picking regression contract passed: ${records.length} real-city fixtures.`);
