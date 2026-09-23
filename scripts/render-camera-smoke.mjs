import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { CityGameRuntime } from "../web/src/wasm.js";

const wasmPath = process.argv[2];
if (!wasmPath) throw new Error("usage: node scripts/render-camera-smoke.mjs <built.wasm>");
const { instance } = await WebAssembly.instantiate(await readFile(wasmPath), {});
const wasm = instance.exports;
const encoder = new TextEncoder();
const decoder = new TextDecoder();
const fixture = JSON.parse(await readFile(new URL("../fixtures/demo-scenario.json", import.meta.url)));
const aspect = 16 / 9;
const views = Array.from({ length: 24 }, (_, index) => ({
  panX: ((index % 9) - 4) / 8,
  panY: ((index % 7) - 3) / 8,
  zoom: 1 + (index % 4),
}));

function invoke(name, values, suffix = []) {
  const inputs = [];
  let output;
  try {
    for (const value of values) {
      const bytes = encoder.encode(JSON.stringify(value));
      const ptr = wasm.city_game_alloc(bytes.length);
      assert.notEqual(ptr, 0);
      inputs.push({ ptr, len: bytes.length });
      new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
    }
    const packed = BigInt.asUintN(64, wasm[name](...inputs.flatMap(({ ptr, len }) => [ptr, len]), ...suffix));
    output = { ptr: Number(packed & 0xffffffffn), len: Number(packed >> 32n) };
    const result = JSON.parse(decoder.decode(new Uint8Array(wasm.memory.buffer, output.ptr, output.len)));
    assert.equal(result.ok, true, result.error);
    return { result, inputBytes: inputs.reduce((sum, value) => sum + value.len, 0), outputBytes: output.len };
  } finally {
    if (output) wasm.city_game_free(output.ptr, output.len);
    for (const input of inputs) wasm.city_game_free(input.ptr, input.len);
  }
}

for (const roadCount of [1, 64, 256]) {
  const scenario = structuredClone(fixture);
  scenario.roads = Array.from({ length: roadCount }, (_, index) => ({
    ...fixture.roads[0],
    id: `imported/way/${100 + index}`,
    sourceId: `way/${100 + index}`,
    geometry: {
      type: "LineString",
      coordinates: fixture.roads[0].geometry.coordinates.map(([lon, lat]) => [
        lon + (index % 32) * 0.001,
        lat + Math.floor(index / 32) * 0.001,
      ]),
    },
  }));
  const stats = { prepare: 0, camera: 0, full: 0, cameraInputBytes: 0, cameraOutputBytes: 0 };
  const runtime = new CityGameRuntime({
    ...wasm,
    city_game_prepare_render(...args) {
      stats.prepare++;
      return wasm.city_game_prepare_render(...args);
    },
    city_game_render_camera(...args) {
      stats.camera++;
      const inputBytes = args[1] + args[3];
      assert.ok(inputBytes <= 2_048, "camera request exceeds the per-view byte budget");
      stats.cameraInputBytes += inputBytes;
      const result = wasm.city_game_render_camera(...args);
      const outputBytes = Number(BigInt.asUintN(64, result) >> 32n);
      assert.ok(outputBytes <= 2_048, "camera response exceeds the per-view byte budget");
      stats.cameraOutputBytes += outputBytes;
      return result;
    },
    city_game_render_frame(...args) {
      stats.full++;
      return wasm.city_game_render_frame(...args);
    },
    city_game_render_frame_view(...args) {
      stats.full++;
      return wasm.city_game_render_frame_view(...args);
    },
  });
  const session = runtime.createSession(scenario);
  const save = invoke("city_game_new_save", [scenario]).result.save;
  const preparationStart = performance.now();
  const first = session.renderFrame(aspect);
  const preparationMs = performance.now() - preparationStart;
  assert.deepEqual(first, invoke("city_game_render_frame", [save], [aspect]).result.frame);

  let legacyInputBytes = 0;
  let legacyOutputBytes = 0;
  const legacyCameras = [];
  const legacyStart = performance.now();
  for (const view of views) {
    const result = invoke("city_game_render_frame_view", [save, view], [aspect]);
    legacyInputBytes += result.inputBytes;
    legacyOutputBytes += result.outputBytes;
    legacyCameras.push(result.result.frame.camera);
  }
  const legacyMs = performance.now() - legacyStart;
  const cachedStart = performance.now();
  const cachedFrames = views.map((view) => session.renderFrame(aspect, view));
  const cachedMs = performance.now() - cachedStart;
  for (let index = 0; index < views.length; index++) {
    assert.strictEqual(cachedFrames[index].nodes, first.nodes);
    assert.deepEqual(cachedFrames[index].camera, legacyCameras[index]);
  }
  assert.equal(stats.prepare, 1, "camera-only navigation must not rebuild the scene");
  assert.equal(stats.camera, views.length);
  assert.equal(stats.full, 0, "the browser must not use legacy full-frame exports");
  // Structural byte budgets, not machine-speed thresholds. Both are independent of node count.
  assert.ok(stats.cameraInputBytes <= views.length * 2_048);
  assert.ok(stats.cameraOutputBytes <= views.length * 2_048);
  assert.ok(stats.cameraInputBytes < legacyInputBytes);
  assert.ok(stats.cameraOutputBytes < legacyOutputBytes);
  assert.strictEqual(session.renderFrame(aspect, { ...views.at(-1) }), cachedFrames.at(-1));
  assert.equal(stats.camera, views.length, "identical views must not cross the WASM boundary");
  console.log(JSON.stringify({
    scenario: "prepared-camera-navigation", roadCount, nodes: first.nodes.length, frames: views.length,
    preparationCount: stats.prepare, fullFrameCallsOnHotPath: stats.full,
    legacyInputBytes, legacyOutputBytes,
    cameraInputBytes: stats.cameraInputBytes, cameraOutputBytes: stats.cameraOutputBytes,
    preparationMs, legacyMs, cachedMs, timing: "advisory-no-wall-clock-gate",
  }));

  // Exercise invalidation using real authoritative commands, not only the unit-test fake.
  const road = {
    id: "player/road/camera-smoke",
    class: "residential",
    geometry: { type: "LineString", coordinates: [[9, 49], [9.01, 49.01]] },
  };
  const addRoad = { kind: "planning", command: { kind: "addRoad", road } };
  session.execute(addRoad);
  const changed = session.renderFrame(aspect);
  assert.equal(stats.prepare, 2);
  assert.ok(changed.nodes.some((node) => node.id.startsWith(road.id + "/")));
  assert.notStrictEqual(changed.nodes, first.nodes);
  const changedSave = invoke("city_game_execute", [save, addRoad]).result.save;
  assert.deepEqual(changed, invoke("city_game_render_frame", [changedSave], [aspect]).result.frame);
  const receipt = session.execute(addRoad);
  assert.equal(receipt.outcome, "unchanged");
  assert.strictEqual(session.renderFrame(aspect), changed);
  assert.throws(() => session.execute({
    kind: "planning", command: { kind: "addRoad", road: { ...road, class: "primary" } },
  }));
  assert.throws(() => session.renderFrame(aspect, { panX: 0, panY: 0, zoom: 0 }));
  session.query({ kind: "timePosition" });
  assert.strictEqual(session.renderFrame(aspect), changed);
  assert.equal(stats.prepare, 2);
  const resized = session.renderFrame(0.75, views[0]);
  assert.equal(stats.prepare, 3);
  assert.deepEqual(resized, invoke("city_game_render_frame_view", [changedSave, views[0]], [0.75]).result.frame);
  session.execute({ kind: "restart" });
  const restarted = session.renderFrame(aspect);
  assert.equal(stats.prepare, 4);
  assert.deepEqual(restarted, first);
}
console.log("prepared camera WASM equivalence, invalidation, and structural budgets passed");
