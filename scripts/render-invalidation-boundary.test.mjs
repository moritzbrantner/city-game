// Acceptance contract for moritzbrantner/city-game#53 (architecture invariant).
// Incremental invalidation is decided in Rust from the authoritative command outcome.
// The browser cache/presentation adapter must never diff old and new scenes semantically:
// across invalidation and re-preparation it may forward node arrays but not read their contents.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

const cacheUrl = new URL("../web/src/render-cache.js", import.meta.url);
const runtimeUrl = new URL("../web/src/wasm.js", import.meta.url);
const cacheSource = readFileSync(cacheUrl, "utf8");

const load = async (text) =>
  (await import(`data:text/javascript;base64,${Buffer.from(text).toString("base64")}`)).CityRenderCache;

function guardedNodes(label) {
  return new Proxy([], {
    get(_target, property) {
      throw new Error(`browser adapter read ${label} node contents (${String(property)})`);
    },
  });
}

function exerciseInvalidation(CityRenderCache) {
  const scenes = [guardedNodes("scene-0"), guardedNodes("scene-1"), guardedNodes("scene-2")];
  let preparations = 0;
  const cache = new CityRenderCache(
    () => ({ frame: { camera: { scene: preparations }, nodes: scenes[preparations++] }, overview: {} }),
    (_overview, view) => ({ ...view }),
  );
  const first = cache.renderFrame(1);
  assert.strictEqual(first.nodes, scenes[0]);
  for (let change = 1; change < scenes.length; change++) {
    cache.invalidate();
    const next = cache.renderFrame(1, { panX: 0.25, panY: 0, zoom: 2 });
    assert.strictEqual(next.nodes, scenes[change], "re-prepared nodes are forwarded unchanged");
    assert.strictEqual(cache.renderFrame(1, { panX: 0.25, panY: 0, zoom: 2 }), next);
  }
  assert.equal(preparations, scenes.length);
}

test("render cache forwards re-prepared scenes without reading or diffing node contents", async () => {
  exerciseInvalidation(await load(cacheSource));
});

test("guard rejects a cache that diffs the previous and next scene in JavaScript", async () => {
  const anchor = "const frame = { camera, nodes: prepared.frame.nodes };";
  assert.ok(cacheSource.includes(anchor), "mutation anchor must exist in the real cache");
  const mutant = cacheSource.replace(
    anchor,
    `${anchor}
    if (globalThis.__cityGamePrevious) {
      const previous = new Set(globalThis.__cityGamePrevious.map((node) => node.id));
      for (const node of prepared.frame.nodes) previous.delete(node.id);
    }
    globalThis.__cityGamePrevious = prepared.frame.nodes;`,
  );
  try {
    await assert.rejects(async () => exerciseInvalidation(await load(mutant)), /node contents/);
  } finally {
    delete globalThis.__cityGamePrevious;
  }
});

test("browser runtime/cache sources do not traverse or key scene nodes", () => {
  for (const url of [cacheUrl, runtimeUrl]) {
    const source = readFileSync(url, "utf8");
    // Pass-through such as `nodes: prepared.frame.nodes` is allowed; element access,
    // array methods, iteration, or id-keyed lookups over scene nodes are not.
    assert.doesNotMatch(source, /\bnodes\s*(\[|\.\s*\w)/, `${url.pathname} indexes or traverses nodes`);
    assert.doesNotMatch(source, /\bof\s+[\w.$#]*nodes\b/, `${url.pathname} iterates nodes`);
  }
});
