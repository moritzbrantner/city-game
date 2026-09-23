import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

const source = await readFile(new URL("../web/src/render-cache.js", import.meta.url), "utf8");
const load = async (text) => (await import(`data:text/javascript;base64,${Buffer.from(text).toString("base64")}`)).CityRenderCache;

function exercise(Cache) {
  let preparations = 0, cameras = 0;
  const cache = new Cache((aspect) => {
    preparations++;
    return { frame: { camera: { aspect }, nodes: [{ id: preparations }] }, overview: { aspect } };
  }, ({ aspect }, view) => { cameras++; return { aspect, zoom: view.zoom }; });
  const initial = cache.renderFrame(1);
  const input = { panX: 0.2, panY: 0.1, zoom: 2 };
  const moved = cache.renderFrame(1, input);
  assert.strictEqual(moved.nodes, initial.nodes, "retained node identity");
  assert.equal(preparations, 1, "one preparation");
  assert.strictEqual(cache.renderFrame(1, { ...input }), moved, "duplicate view does no work");
  assert.equal(cameras, 1);
  input.zoom = 3;
  assert.equal(cache.renderFrame(1, input).camera.zoom, 3, "snapshot caller-owned view");
  const resized = cache.renderFrame(2, input);
  assert.equal(resized.camera.aspect, 2, "aspect invalidation");
  cache.invalidate();
  assert.notStrictEqual(cache.renderFrame(2, input).nodes, resized.nodes, "mutation invalidation");
}

test("mutation-test oracle accepts the unmodified production cache", async () => exercise(await load(source)));
for (const [name, before, after] of [
  ["rebuilding every view", "this.#prepared = prepared;", "this.#prepared = null;"],
  ["copying every node array", "nodes: prepared.frame.nodes", "nodes: [...prepared.frame.nodes]"],
  ["recomputing identical views", "if (sameAspect && sameView(view, this.#view))", "if (false)"],
  ["retaining stale geometry after mutation", "invalidate() {", "invalidate() { return;"],
  ["ignoring aspect changes", "aspect === this.#aspect", "true"],
  ["retaining caller-owned view objects", "this.#view = { panX: view.panX, panY: view.panY, zoom: view.zoom };", "this.#view = view;"],
]) {
  test(`regression oracle kills actual cache mutant: ${name}`, async () => {
    assert.ok(source.includes(before), `mutation anchor moved: ${name}; update the regression test explicitly`);
    // Import outside assert.throws: syntax/import failures do not count as a detected regression.
    const Mutant = await load(source.replace(before, after));
    assert.throws(() => exercise(Mutant), assert.AssertionError);
  });
}
