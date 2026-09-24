import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  CityPickingIndex,
  entityScreenBoundsLinear,
  pickNodeLinear,
} from "../web/src/picking-index.js";

const budget = JSON.parse(
  readFileSync(new URL("../.performance/picking-budget.json", import.meta.url), "utf8"),
);

const OVERVIEW = Object.freeze({ panX: 0, panY: 0, zoom: 1 });

function camera(view) {
  return {
    aspect: 1,
    viewMatrix: [
      1, 0, 0, 0,
      0, 1, 0, 0,
      0, 0, 1, 0,
      0, 0, 0, 1,
    ],
    projectionMatrix: [
      view.zoom, 0, 0, 0,
      0, view.zoom, 0, 0,
      0, 0, 1, 0,
      -2 * view.zoom * view.panX,
      -2 * view.zoom * view.panY,
      0,
      1,
    ],
  };
}

function frame(nodes, view) {
  return { camera: camera(view), nodes };
}

function gridNodes(count) {
  const columns = Math.ceil(Math.sqrt(count));
  const rows = Math.ceil(count / columns);
  const nodes = [];
  for (let index = 0; index < count; index++) {
    const column = index % columns;
    const row = Math.floor(index / columns);
    const x = -0.95 + (column / Math.max(1, columns - 1)) * 1.9;
    const y = -0.95 + (row / Math.max(1, rows - 1)) * 1.9;
    const angle = index % 7 === 0 ? 0.2 : 0;
    nodes.push({
      id: index % 5 === 0 ? `road/${index}/road-segment-0` : `building/${index}`,
      geometry: { kind: "box", size: [0.012, 0.012, 0.06] },
      color: 0xffffff,
      transform: {
        translation: [x, y, 0.5],
        rotationQuaternion: [0, 0, Math.sin(angle / 2), Math.cos(angle / 2)],
      },
    });
  }
  return nodes;
}

function kindForEntity(id) {
  if (id.startsWith("road/")) return "Road";
  if (id.startsWith("building/")) return "Building";
  return undefined;
}

function pointer(x, y, pointerType = "mouse") {
  return { x, y, width: 1280, height: 720, pointerType };
}

test("indexed picking preserves the linear reference across pan, zoom, and empty space", () => {
  const nodes = gridNodes(2_048);
  const buildView = { panX: 0.2, panY: -0.15, zoom: 2 };
  const index = new CityPickingIndex(frame(nodes, buildView), buildView);
  const views = [
    OVERVIEW,
    { panX: 0.25, panY: -0.125, zoom: 2 },
    { panX: -0.7, panY: 0.5, zoom: 0.5 },
    { panX: 0.8, panY: -0.4, zoom: 8 },
  ];
  const pointers = [];
  for (let row = 0; row < 4; row++) {
    for (let column = 0; column < 5; column++) {
      pointers.push(pointer(40 + column * 290, 35 + row * 210, (row + column) % 4 === 0 ? "touch" : "mouse"));
    }
  }

  for (const view of views) {
    const current = frame(nodes, view);
    for (const input of pointers) {
      const fast = index.pick(current, view, input, kindForEntity);
      const reference = pickNodeLinear(current, input, kindForEntity);
      assert.equal(fast.node?.id ?? null, reference.node?.id ?? null);
      assert.equal(fast.observations.path, "indexed");
      assert.equal(fast.observations.fullSceneNodeVisits, 0);
      assert.ok(
        fast.observations.candidateBoundsEvaluations <= budget.maxCandidateBoundsEvaluationsPerPick,
        JSON.stringify(fast.observations),
      );
    }
  }
});

test("21k-node stress queries stay bounded after one index build", () => {
  const nodes = gridNodes(budget.stressNodes);
  const initial = frame(nodes, OVERVIEW);
  const buildStart = performance.now();
  const index = new CityPickingIndex(initial, OVERVIEW);
  const buildMs = performance.now() - buildStart;

  assert.equal(index.buildObservations.nodeVisits, nodes.length);
  assert.equal(
    index.buildObservations.nodeVisits,
    nodes.length * budget.buildNodeVisitsPerSceneNode,
  );
  assert.equal(
    index.buildObservations.projectionCalls,
    index.buildObservations.indexedNodes * budget.buildProjectionCallsPerIndexedBox,
  );

  let maxBounds = 0;
  let maxProjectionCalls = 0;
  let totalBounds = 0;
  const queryStart = performance.now();
  for (let indexValue = 0; indexValue < budget.stressQueries; indexValue++) {
    const view = [
      OVERVIEW,
      { panX: 0.25, panY: -0.125, zoom: 2 },
      { panX: -0.5, panY: 0.5, zoom: 0.5 },
      { panX: 0.75, panY: -0.75, zoom: 8 },
    ][indexValue % 4];
    const current = frame(nodes, view);
    const input = pointer(
      (indexValue * 811) % 1280,
      (indexValue * 421) % 720,
      indexValue % 5 === 0 ? "touch" : "mouse",
    );
    const result = index.pick(current, view, input, kindForEntity);
    assert.equal(result.observations.path, "indexed");
    assert.equal(result.observations.fullSceneNodeVisits, budget.maxFullSceneFallbackNodeVisits);
    maxBounds = Math.max(maxBounds, result.observations.candidateBoundsEvaluations);
    maxProjectionCalls = Math.max(maxProjectionCalls, result.observations.projectionCalls);
    totalBounds += result.observations.candidateBoundsEvaluations;
  }
  const queryMs = performance.now() - queryStart;

  assert.ok(maxBounds <= budget.maxCandidateBoundsEvaluationsPerPick, `max exact candidates: ${maxBounds}`);
  assert.ok(maxProjectionCalls <= budget.maxCandidateProjectionCallsPerPick, `max projection calls: ${maxProjectionCalls}`);
  console.log(JSON.stringify({
    benchmark: "picking-index-structural",
    nodes: nodes.length,
    queries: budget.stressQueries,
    indexBuildNodeVisits: index.buildObservations.nodeVisits,
    indexBuildProjectionCalls: index.buildObservations.projectionCalls,
    cellReferences: index.buildObservations.cellReferences,
    wideItems: index.buildObservations.wideItems,
    maxCandidateBoundsEvaluations: maxBounds,
    averageCandidateBoundsEvaluations: totalBounds / budget.stressQueries,
    maxProjectionCallsPerPick: maxProjectionCalls,
    buildMs,
    queryMs,
    timing: "advisory-no-wall-clock-gate",
  }));
});

test("unsupported camera-family changes fall back to the exact linear reference", () => {
  const nodes = gridNodes(128);
  const index = new CityPickingIndex(frame(nodes, OVERVIEW), OVERVIEW);
  const changed = frame(nodes, OVERVIEW);
  changed.camera.viewMatrix = [...changed.camera.viewMatrix];
  changed.camera.viewMatrix[12] = 0.2;
  const input = pointer(640, 360);

  const fast = index.pick(changed, OVERVIEW, input, kindForEntity);
  const reference = pickNodeLinear(changed, input, kindForEntity);
  assert.equal(fast.node?.id ?? null, reference.node?.id ?? null);
  assert.equal(fast.observations.path, "linear-fallback");
  assert.equal(fast.observations.fullSceneNodeVisits, nodes.length);
});

test("selected-entity bounds project only that entity instead of rescanning the scene", () => {
  const nodes = gridNodes(10_000);
  nodes.push(
    {
      id: "road/selected/road-segment-0",
      geometry: { kind: "box", size: [0.08, 0.02, 0.02] },
      color: 0xffffff,
      transform: { translation: [-0.15, 0.1, 0.5] },
    },
    {
      id: "road/selected/road-segment-1",
      geometry: { kind: "box", size: [0.08, 0.02, 0.02] },
      color: 0xffffff,
      transform: { translation: [-0.07, 0.1, 0.5] },
    },
  );
  const view = { panX: 0.1, panY: 0.05, zoom: 4 };
  const current = frame(nodes, view);
  const index = new CityPickingIndex(current, view);

  const fast = index.screenBoundsForEntity(current, view, "road/selected", 1280, 720);
  const reference = entityScreenBoundsLinear(current, "road/selected", 1280, 720);
  assert.deepEqual(fast.bounds, reference.bounds);
  assert.equal(fast.observations.path, "indexed");
  assert.equal(fast.observations.nodeVisits, 2);
  assert.equal(reference.observations.nodeVisits, nodes.length);
});

test("large scene-covering boxes stay correct without exploding grid references", () => {
  const nodes = gridNodes(512);
  nodes.push({
    id: "water/whole-city",
    geometry: { kind: "box", size: [1.9, 1.9, 0.02] },
    color: 0xffffff,
    opacity: 0.4,
    transform: { translation: [0, 0, 0.7] },
  });
  const index = new CityPickingIndex(frame(nodes, OVERVIEW), OVERVIEW);
  assert.ok(index.buildObservations.wideItems >= 1);
  const input = pointer(640, 360);
  const fast = index.pick(frame(nodes, OVERVIEW), OVERVIEW, input, kindForEntity);
  const reference = pickNodeLinear(frame(nodes, OVERVIEW), input, kindForEntity);
  assert.equal(fast.node?.id ?? null, reference.node?.id ?? null);
});

test("invalid view and pointer inputs fail before returning a misleading pick", () => {
  const nodes = gridNodes(16);
  assert.throws(
    () => new CityPickingIndex(frame(nodes, OVERVIEW), { ...OVERVIEW, zoom: 0 }),
    /positive zoom/,
  );
  const index = new CityPickingIndex(frame(nodes, OVERVIEW), OVERVIEW);
  assert.throws(
    () => index.pick(frame(nodes, OVERVIEW), OVERVIEW, { x: 0, y: 0, width: 0, height: 1 }),
    /positive viewport/,
  );
});
