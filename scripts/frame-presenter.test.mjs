import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import {
  CityFramePresenter,
  entityIdForNode,
  validateSessionFrame,
} from "../web/src/frame-presenter.js";

const budget = JSON.parse(
  readFileSync(new URL("../.performance/navigation-budget.json", import.meta.url), "utf8"),
).presentation;

function camera(seed = 1) {
  return {
    aspect: 1,
    viewMatrix: [seed, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1],
    projectionMatrix: [seed, 0, 0, 0, 0, seed, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1],
  };
}

function rendererHarness() {
  const work = { sceneCalls: 0, cameraCalls: 0, nodeVisits: 0 };
  return {
    work,
    renderer: {
      render(frame) {
        work.sceneCalls++;
        for (const _node of frame.nodes) work.nodeVisits++;
        return { nodeVisitCount: frame.nodes.length };
      },
      renderCamera() {
        work.cameraCalls++;
        return { nodeVisitCount: 0 };
      },
    },
  };
}

test("camera-only presentation stays O(1) after a 21k-node scene submission", () => {
  let nodeReads = 0;
  const source = Array.from({ length: budget.nodes }, (_, index) => ({
    id: `building/${index}`,
    geometry: { kind: "box", size: [1, 1, 1] },
    color: 0xffffff,
    transform: { translation: [index, 0, 0] },
  }));
  const nodes = new Proxy(source, {
    get(target, property, receiver) {
      if (property === Symbol.iterator || (typeof property === "string" && /^\d+$/.test(property))) {
        nodeReads++;
      }
      return Reflect.get(target, property, receiver);
    },
  });
  const { renderer, work } = rendererHarness();
  const presenter = new CityFramePresenter(renderer);
  presenter.render({ camera: camera(), nodes });
  const readsAfterScene = nodeReads;

  for (let index = 1; index <= budget.cameraFrames; index++) {
    const result = presenter.render({ camera: camera(index + 1), nodes });
    assert.equal(result.path, "camera");
    assert.equal(result.observations.nodeVisitCount, budget.maxCameraOnlyRendererNodeVisits);
    assert.strictEqual(result.frame.nodes, nodes);
  }

  assert.equal(work.sceneCalls, budget.maxFullSceneSubmissions);
  assert.equal(work.cameraCalls, budget.cameraFrames);
  assert.equal(work.nodeVisits, budget.nodes);
  assert.ok(
    nodeReads - readsAfterScene <= budget.maxNodeReadsAfterInitialSubmission,
    "camera path traversed retained scene nodes",
  );
});

test("selection decoration is materialized once and reused across camera movement", () => {
  let iterations = 0;
  const source = [
    {
      id: "road/1/road-segment-0",
      geometry: { kind: "box", size: [1, 1, 1] },
      color: 0x111111,
      transform: { translation: [0, 0, 0] },
    },
    {
      id: "building/2",
      geometry: { kind: "box", size: [1, 1, 1] },
      color: 0x222222,
      transform: { translation: [2, 0, 0] },
    },
  ];
  const nodes = new Proxy(source, {
    get(target, property, receiver) {
      if (property === Symbol.iterator) iterations++;
      return Reflect.get(target, property, receiver);
    },
  });
  const { renderer, work } = rendererHarness();
  const presenter = new CityFramePresenter(renderer);

  const selected = presenter.render({ camera: camera(), nodes }, "road/1", 0xd17f00);
  assert.equal(selected.path, "scene");
  assert.equal(selected.frame.nodes.length, 3);
  const decorated = selected.frame.nodes;
  const iterationsAfterSelection = iterations;

  for (let index = 0; index < 1_000; index++) {
    const next = presenter.render({ camera: camera(index + 2), nodes }, "road/1", 0xd17f00);
    assert.equal(next.path, "camera");
    assert.strictEqual(next.frame.nodes, decorated);
  }
  assert.equal(iterations, iterationsAfterSelection, "selection overlay was rebuilt on camera movement");

  const recolored = presenter.render({ camera: camera(2000), nodes }, "road/1", 0x0072b2);
  assert.equal(recolored.path, "scene");
  assert.notStrictEqual(recolored.frame.nodes, decorated);

  const cleared = presenter.render({ camera: camera(2001), nodes });
  assert.equal(cleared.path, "scene");
  assert.strictEqual(cleared.frame.nodes, nodes);
  assert.deepEqual(
    { sceneCalls: work.sceneCalls, cameraCalls: work.cameraCalls },
    { sceneCalls: 3, cameraCalls: 1_000 },
  );
});

test("frame validation skips retained nodes but fails closed on a changed scene", () => {
  const nodes = [{ id: "one" }];
  let fullCalls = 0;
  let cameraCalls = 0;
  const full = (frame) => {
    fullCalls++;
    for (const node of frame.nodes) assert.ok(node.id);
    return frame;
  };
  const cameraOnly = (value) => {
    cameraCalls++;
    assert.equal(value.projectionMatrix.length, 16);
    return value;
  };

  const first = validateSessionFrame(null, { camera: camera(), nodes }, full, cameraOnly);
  const second = validateSessionFrame(first, { camera: camera(2), nodes }, full, cameraOnly);
  assert.equal(fullCalls, 1);
  assert.equal(cameraCalls, 1);
  assert.strictEqual(second.nodes, nodes);

  validateSessionFrame(second, { camera: camera(3), nodes: [...nodes] }, full, cameraOnly);
  assert.equal(fullCalls, 2);
  assert.equal(cameraCalls, 1);
});

test("renderer replacement and scene mutation force exactly one full resubmission", () => {
  const firstHarness = rendererHarness();
  const secondHarness = rendererHarness();
  const presenter = new CityFramePresenter(firstHarness.renderer);
  const nodes = [{ id: "building/1" }];

  presenter.render({ camera: camera(), nodes });
  presenter.render({ camera: camera(2), nodes });
  presenter.setRenderer(secondHarness.renderer);
  presenter.render({ camera: camera(3), nodes });
  presenter.render({ camera: camera(4), nodes });
  const changedNodes = [...nodes];
  presenter.render({ camera: camera(5), nodes: changedNodes });
  presenter.render({ camera: camera(6), nodes: changedNodes });

  assert.deepEqual(firstHarness.work, { sceneCalls: 1, cameraCalls: 1, nodeVisits: 1 });
  assert.deepEqual(secondHarness.work, { sceneCalls: 2, cameraCalls: 2, nodeVisits: 2 });
});

test("entity id mapping preserves road segment ownership", () => {
  assert.equal(entityIdForNode("imported/way/10/road-segment-27"), "imported/way/10");
  assert.equal(entityIdForNode("imported/way/20"), "imported/way/20");
});
