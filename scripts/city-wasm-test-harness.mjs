import assert from "node:assert/strict";
import { CityGameRuntime } from "../web/src/wasm.js";

const encoder = new TextEncoder();
const decoder = new TextDecoder();
const unpack = (value) => {
  const packed = BigInt.asUintN(64, value);
  return { ptr: Number(packed & 0xffffffffn), len: Number(packed >> 32n) };
};

export function harness(wasm) {
  const live = new Map();
  const sessions = new Set();
  let stats;
  const reset = () => {
    assert.equal(live.size, 0, "unfreed allocation at journey boundary");
    stats = {
      preparations: 0,
      cameraCalls: 0,
      fullFrameCalls: 0,
      otherCalls: 0,
      inputBytes: 0,
      outputBytes: 0,
      maxInputBytes: 0,
      maxOutputBytes: 0,
      sessionCreates: 0,
      sessionDestroys: 0,
      sessionExecuteCalls: 0,
      sessionQueryCalls: 0,
      sessionCreateInputBytes: 0,
      sessionOperationInputBytes: 0,
      sessionOperationOutputBytes: 0,
      maxSessionOperationInputBytes: 0,
      maxSessionOperationOutputBytes: 0,
    };
  };
  reset();

  const remember = (ptr, len) => {
    assert.ok(ptr > 0 && len > 0);
    assert.equal(live.has(ptr), false, "allocation overlaps a live input/output");
    live.set(ptr, len);
  };
  const decodeOutput = (output) =>
    JSON.parse(decoder.decode(new Uint8Array(wasm.memory.buffer, output.ptr, output.len)));

  const exports = {
    ...wasm,
    city_game_alloc(len) {
      const ptr = wasm.city_game_alloc(len);
      if (ptr && len) remember(ptr, len);
      return ptr;
    },
    city_game_free(ptr, len) {
      if (ptr && len) {
        assert.equal(live.get(ptr), len, "double free or incorrect allocation length");
        live.delete(ptr);
      }
      wasm.city_game_free(ptr, len);
    },
    city_game_session_destroy(handle) {
      assert.equal(sessions.delete(handle), true, "destroyed unknown or already-destroyed session");
      stats.sessionDestroys++;
      wasm.city_game_session_destroy(handle);
    },
  };

  for (const name of Object.keys(wasm).filter((name) =>
    name.startsWith("city_game_") &&
      !["city_game_alloc", "city_game_free", "city_game_session_destroy"].includes(name))) {
    exports[name] = (...args) => {
      const result = wasm[name](...args);
      const output = unpack(result);
      remember(output.ptr, output.len);

      if (name === "city_game_session_create") {
        stats.sessionCreates++;
        stats.sessionCreateInputBytes += args[1];
        const response = decodeOutput(output);
        if (response.ok === true) sessions.add(response.handle);
      } else if (name === "city_game_session_execute" || name === "city_game_session_query") {
        const inputBytes = args[2];
        if (name === "city_game_session_execute") stats.sessionExecuteCalls++;
        else stats.sessionQueryCalls++;
        stats.sessionOperationInputBytes += inputBytes;
        stats.sessionOperationOutputBytes += output.len;
        stats.maxSessionOperationInputBytes = Math.max(stats.maxSessionOperationInputBytes, inputBytes);
        stats.maxSessionOperationOutputBytes = Math.max(stats.maxSessionOperationOutputBytes, output.len);
      } else if (name === "city_game_session_prepare_render" || name === "city_game_prepare_render") {
        stats.preparations++;
      } else if (name === "city_game_render_camera") {
        const inputBytes = args[1] + args[3];
        stats.cameraCalls++;
        stats.inputBytes += inputBytes;
        stats.outputBytes += output.len;
        stats.maxInputBytes = Math.max(stats.maxInputBytes, inputBytes);
        stats.maxOutputBytes = Math.max(stats.maxOutputBytes, output.len);
      } else if (name.startsWith("city_game_render_frame")) {
        stats.fullFrameCalls++;
      } else {
        stats.otherCalls++;
      }
      return result;
    };
  }

  return {
    runtime: new CityGameRuntime(exports),
    reset,
    snapshot: () => ({
      ...stats,
      liveAllocations: live.size,
      liveSessions: sessions.size,
    }),
  };
}

// Independent full-save/full-frame reference. It deliberately bypasses instrumentation
// on the production wrapper so oracle work is not mistaken for application work.
export function call(wasm, name, values, suffix = []) {
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
    output = unpack(wasm[name](...inputs.flatMap(({ ptr, len }) => [ptr, len]), ...suffix));
    const response = JSON.parse(decoder.decode(new Uint8Array(wasm.memory.buffer, output.ptr, output.len)));
    assert.equal(response.ok, true, response.error);
    return response;
  } finally {
    if (output) wasm.city_game_free(output.ptr, output.len);
    for (const input of inputs) wasm.city_game_free(input.ptr, input.len);
  }
}
