import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { CityRenderCache } from "../web/src/render-cache.js";
import { checkCoverage, checkNavigation, summarize } from "./navigation-contract.mjs";

const contract = JSON.parse(readFileSync(new URL("../.performance/navigation-budget.json", import.meta.url)));
const budget = { ...contract, ...contract.fixtures["roads-256"] };
const good = {
  frames: 24, preparations: 1, cameraCalls: 24, fullFrameCalls: 0, otherCalls: 0,
  liveAllocations: 0, maxInputBytes: 352, maxOutputBytes: 512,
  inputBytes: 6770, outputBytes: 10938,
};

test("navigation budget accepts the retained baseline", () => checkNavigation(good, budget));
for (const [metric, value] of Object.entries({
  frames: 23, preparations: 2, cameraCalls: 25, fullFrameCalls: 1, otherCalls: 1,
  liveAllocations: 1, maxInputBytes: 353, maxOutputBytes: 513,
  inputBytes: 6771, outputBytes: 10939,
})) {
  test(`ratchet rejects deliberate regression: ${metric}`, () => {
    assert.throws(() => checkNavigation({ ...good, [metric]: value }, budget), assert.AssertionError);
  });
}

test("benchmark cannot pass by silently dropping or duplicating a scenario", () => {
  const expected = Object.keys(contract.fixtures);
  const records = expected.map((id) => ({ id }));
  checkCoverage(records, expected);
  assert.throws(() => checkCoverage(records.slice(1), expected));
  assert.throws(() => checkCoverage([...records, records[0]], expected));
  assert.throws(() => checkCoverage([...records, { id: "unknown" }], expected));
});

test("camera navigation never reads, iterates, copies, or decorates retained nodes", () => {
  // Even a linear scan that preserves identity must trip this guard.
  const nodes = new Proxy([], { get() { throw new Error("node traversal on camera-only path"); } });
  let preparations = 0;
  const cache = new CityRenderCache(
    () => { preparations++; return { frame: { camera: {}, nodes }, overview: {} }; },
    () => ({}),
  );
  const initial = cache.renderFrame(1);
  for (let index = 1; index <= 10_000; index++) {
    const frame = cache.renderFrame(1, { panX: (index % 17) / 4, panY: 0, zoom: 2 });
    assert.strictEqual(frame.nodes, nodes);
  }
  assert.strictEqual(cache.renderFrame(1).camera, initial.camera);
  assert.equal(preparations, 1);
});

test("timing summaries retain distributions without mutating samples", () => {
  const samples = [7, 1, 5, 3, 4, 2, 6];
  assert.deepEqual(summarize(samples), { samples: 7, medianMs: 4, p95Ms: 7, minMs: 1, maxMs: 7 });
  assert.deepEqual(samples, [7, 1, 5, 3, 4, 2, 6]);
  assert.throws(() => summarize([]));
  assert.throws(() => summarize([NaN]));
});
