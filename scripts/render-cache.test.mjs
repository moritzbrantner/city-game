import assert from "node:assert/strict";
import test from "node:test";
import { CityRenderCache } from "../web/src/render-cache.js";
import { CityGameRuntime } from "../web/src/wasm.js";

const overview = { panX: 0, panY: 0, zoom: 1 };
const moved = { panX: 0.25, panY: -0.125, zoom: 2 };

function cacheHarness(nodeCount = 1) {
  const calls = { prepare: 0, camera: 0 };
  let rejectPreparation = false;
  const cache = new CityRenderCache(
    (aspect) => {
      calls.prepare++;
      if (rejectPreparation) throw new Error("invalid preparation");
      return {
        frame: {
          camera: { aspect, ...overview },
          nodes: Array.from({ length: nodeCount }, (_, id) => ({ id })),
        },
        overview: { aspect },
      };
    },
    (prepared, view) => {
      calls.camera++;
      if (!Number.isFinite(view.zoom) || view.zoom <= 0) throw new Error("invalid zoom");
      return { aspect: prepared.aspect, ...view };
    },
  );
  return { cache, calls, rejectPreparation: (value) => { rejectPreparation = value; } };
}

test("camera work is independent of scene size and keeps node identity", () => {
  for (const count of [0, 1, 10_000]) {
    const { cache, calls } = cacheHarness(count);
    const first = cache.renderFrame(1);
    for (let step = 1; step <= 256; step++) {
      const next = cache.renderFrame(1, { panX: step / 256, panY: 0, zoom: 2 });
      assert.strictEqual(next.nodes, first.nodes);
      if (count > 0) assert.strictEqual(next.nodes[0], first.nodes[0]);
    }
    assert.deepEqual(calls, { prepare: 1, camera: 256 });
  }
});

test("duplicate views do no work and mutable view inputs are snapshotted", () => {
  const { cache, calls } = cacheHarness();
  const view = { ...moved };
  const first = cache.renderFrame(1, view);
  assert.strictEqual(cache.renderFrame(1, { ...view }), first);
  view.zoom = 3;
  const second = cache.renderFrame(1, view);
  assert.notStrictEqual(second, first);
  assert.equal(second.camera.zoom, 3);
  assert.deepEqual(calls, { prepare: 1, camera: 2 });
});

test("overview and null views reuse the original fitted camera without drift", () => {
  const { cache, calls } = cacheHarness();
  const first = cache.renderFrame(1);
  cache.renderFrame(1, moved);
  const restored = cache.renderFrame(1, null);
  assert.strictEqual(restored.camera, first.camera);
  assert.strictEqual(restored.nodes, first.nodes);
  assert.strictEqual(cache.renderFrame(1, { ...overview }), restored);
  assert.deepEqual(calls, { prepare: 1, camera: 1 });
});

test("aspect changes and invalidation reprepare exactly once", () => {
  const { cache, calls } = cacheHarness();
  const first = cache.renderFrame(1);
  const resized = cache.renderFrame(2, moved);
  assert.notStrictEqual(resized.nodes, first.nodes);
  assert.strictEqual(cache.renderFrame(2, moved), resized);
  cache.invalidate();
  const changed = cache.renderFrame(2, moved);
  assert.notStrictEqual(changed.nodes, resized.nodes);
  assert.deepEqual(calls, { prepare: 3, camera: 2 });
});

test("invalid camera/aspect/preparation leaves the previous successful cache intact", () => {
  const { cache, rejectPreparation } = cacheHarness();
  const first = cache.renderFrame(1);
  for (const aspect of [0, -1, Infinity, NaN]) {
    assert.throws(() => cache.renderFrame(aspect), /aspect/);
  }
  assert.throws(() => cache.renderFrame(1, { ...moved, zoom: 0 }), /zoom/);
  assert.throws(() => cache.renderFrame(2, { ...moved, zoom: 0 }), /zoom/);
  rejectPreparation(true);
  assert.throws(() => cache.renderFrame(2), /preparation/);
  assert.strictEqual(cache.renderFrame(1), first);
});

test("failed rebuild after invalidation cannot resurrect stale geometry", () => {
  const { cache, rejectPreparation } = cacheHarness();
  const first = cache.renderFrame(1);
  cache.invalidate();
  rejectPreparation(true);
  assert.throws(() => cache.renderFrame(1), /preparation/);
  rejectPreparation(false);
  assert.notStrictEqual(cache.renderFrame(1).nodes, first.nodes);
});

// Contract fake for wrapper ownership/invalidation tests, not a performance or Rust oracle.
function runtimeHarness() {
  const memory = new WebAssembly.Memory({ initial: 16 });
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  const allocations = new Map();
  const sessions = new Map();
  const calls = {
    prepare: 0,
    camera: 0,
    full: 0,
    alloc: 0,
    created: 0,
    destroyed: 0,
    createInputBytes: 0,
    commandInputBytes: 0,
    queryInputBytes: 0,
    prepareInputBytes: 0,
  };
  let next = 8;
  let nextSession = 1;
  let failAllocation = Infinity;
  const allocate = (len) => {
    calls.alloc++;
    if (calls.alloc === failAllocation) return 0;
    const ptr = next;
    next += len + 8;
    allocations.set(ptr, len);
    return ptr;
  };
  const read = (ptr, len) => JSON.parse(decoder.decode(new Uint8Array(memory.buffer, ptr, len)));
  const response = (value) => {
    const bytes = encoder.encode(JSON.stringify(value));
    const ptr = allocate(bytes.length);
    new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
    return (BigInt(bytes.length) << 32n) | BigInt(ptr);
  };
  const requireSession = (handle) => {
    const session = sessions.get(handle);
    if (!session) throw new Error("invalid fake session handle");
    return session;
  };
  const exports = {
    memory,
    city_game_alloc: allocate,
    city_game_free(ptr, len) {
      assert.equal(allocations.get(ptr), len, "free must match a live allocation");
      allocations.delete(ptr);
    },
    city_game_session_create(ptr, len) {
      calls.created++;
      calls.createInputBytes += len;
      const handle = nextSession++;
      sessions.set(handle, { scenario: read(ptr, len), revision: 0 });
      return response({ ok: true, handle });
    },
    city_game_session_destroy(handle) {
      assert.equal(sessions.delete(handle), true, "destroy must own a live session");
      calls.destroyed++;
    },
    city_game_session_execute(handle, commandPtr, commandLen) {
      calls.commandInputBytes += commandLen;
      const save = requireSession(handle);
      const command = read(commandPtr, commandLen);
      if (command.kind === "reject") return response({ ok: false, error: "rejected" });
      if (command.kind === "unchanged") {
        return response({ ok: true, outcome: { kind: "planning", outcome: "unchanged" } });
      }
      save.revision++;
      return response({ ok: true, outcome: { kind: command.kind } });
    },
    city_game_session_query(handle, queryPtr, queryLen) {
      calls.queryInputBytes += queryLen;
      requireSession(handle);
      read(queryPtr, queryLen);
      return response({ ok: true, result: { kind: "timePosition" } });
    },
    city_game_session_prepare_render(handle, aspect) {
      calls.prepare++;
      calls.prepareInputBytes += 0;
      const save = requireSession(handle);
      return response({
        ok: true,
        frame: { camera: { aspect }, nodes: [{ revision: save.revision }] },
        overview: { aspect },
      });
    },
    city_game_render_camera(ptr, len, viewPtr, viewLen) {
      calls.camera++;
      return response({ ok: true, camera: { ...read(ptr, len), ...read(viewPtr, viewLen) } });
    },
  };
  return {
    runtime: new CityGameRuntime(exports),
    calls,
    allocations,
    sessions,
    failNextSecondInput() {
      failAllocation = calls.alloc + 2;
    },
  };
}

test("successful mutations/restart invalidate, rejected/no-op commands and queries do not", () => {
  const { runtime, calls, allocations, sessions } = runtimeHarness();
  const session = runtime.createSession({ name: "test" });
  const first = session.renderFrame(1);
  session.query({ kind: "timePosition" });
  assert.throws(() => session.execute({ kind: "reject" }), /rejected/);
  session.execute({ kind: "unchanged" });
  assert.strictEqual(session.renderFrame(1), first);
  session.renderFrame(1, moved);
  assert.equal(calls.prepare, 1);
  for (const kind of ["planning", "restarted", "future-command"]) {
    session.execute({ kind });
    const rebuilt = session.renderFrame(1);
    assert.notStrictEqual(rebuilt.nodes, first.nodes);
  }
  assert.deepEqual({ prepare: calls.prepare, full: calls.full }, { prepare: 4, full: 0 });
  assert.equal(allocations.size, 0);
  session.dispose();
  assert.equal(sessions.size, 0);
});

test("session caches and authoritative saves are isolated", () => {
  const { runtime, calls, sessions } = runtimeHarness();
  const left = runtime.createSession({ name: "left" });
  const right = runtime.createSession({ name: "right" });
  const leftFrame = left.renderFrame(1);
  const rightFrame = right.renderFrame(1);
  assert.notStrictEqual(leftFrame.nodes, rightFrame.nodes);
  left.execute({ kind: "planning" });
  left.renderFrame(1);
  assert.strictEqual(right.renderFrame(1), rightFrame);
  assert.equal(calls.prepare, 3);
  left.dispose();
  assert.equal(sessions.size, 1);
  right.dispose();
  assert.equal(sessions.size, 0);
});

test("partial camera input allocation failure frees earlier WASM inputs", () => {
  const { runtime, allocations, failNextSecondInput } = runtimeHarness();
  const session = runtime.createSession({ name: "test" });
  const first = session.renderFrame(1);
  failNextSecondInput();
  assert.throws(() => session.renderFrame(1, moved), /allocate/);
  assert.equal(allocations.size, 0);
  assert.strictEqual(session.renderFrame(1), first);
  session.dispose();
});

test("partial JSON serialization failure leaves retained session usable", () => {
  const { runtime, allocations } = runtimeHarness();
  const session = runtime.createSession({ name: "test" });
  const circular = {};
  circular.self = circular;
  assert.throws(() => session.execute(circular), /circular/i);
  assert.equal(allocations.size, 0);
  assert.deepEqual(session.query({ kind: "timePosition" }), { kind: "timePosition" });
  session.dispose();
});

test("post-create state operations do not serialize the retained save", () => {
  const { runtime, calls } = runtimeHarness();
  const scenario = { name: "large", payload: "x".repeat(100_000) };
  const session = runtime.createSession(scenario);
  const command = { kind: "unchanged" };
  const query = { kind: "timePosition" };

  session.execute(command);
  session.query(query);
  session.renderFrame(1);

  assert.ok(calls.createInputBytes > 100_000);
  assert.equal(calls.commandInputBytes, new TextEncoder().encode(JSON.stringify(command)).length);
  assert.equal(calls.queryInputBytes, new TextEncoder().encode(JSON.stringify(query)).length);
  assert.equal(calls.prepareInputBytes, 0);
  session.dispose();
});

test("session disposal is idempotent and rejects later use", () => {
  const { runtime, calls, sessions } = runtimeHarness();
  const session = runtime.createSession({ name: "test" });
  session.dispose();
  session.dispose();
  assert.equal(calls.destroyed, 1);
  assert.equal(sessions.size, 0);
  assert.throws(() => session.execute({ kind: "planning" }), /disposed/);
  assert.throws(() => session.query({ kind: "timePosition" }), /disposed/);
  assert.throws(() => session.renderFrame(1), /disposed/);
});
