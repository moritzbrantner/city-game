// Test-only observation installed through CDP before the real application starts.
// Capture GPU projection uniforms as well as WASM calls: a restored cached overview
// need not cross WASM, so the last WASM reply is not necessarily the displayed camera.
window.__cityEvidence = { calls: {}, uploads: 0, pointerId: null, camera: null, nodes: null, projection: null };
const evidence = window.__cityEvidence;
addEventListener("pointerdown", (event) => { evidence.pointerId = event.pointerId; }, true);
const instantiate = WebAssembly.instantiate;
WebAssembly.instantiate = async function (...args) {
  const result = await instantiate.apply(this, args);
  const instance = result.instance ?? result;
  const exports = { ...instance.exports };
  for (const [name, value] of Object.entries(exports)) {
    if (typeof value !== "function" || !name.startsWith("city_game_") || /_(alloc|free)$/.test(name)) continue;
    exports[name] = (...args) => {
      evidence.calls[name] = (evidence.calls[name] ?? 0) + 1;
      const packed = value(...args);
      const unsigned = BigInt.asUintN(64, packed);
      const response = JSON.parse(new TextDecoder().decode(new Uint8Array(exports.memory.buffer,
        Number(unsigned & 0xffffffffn), Number(unsigned >> 32n))));
      if (response.frame) { evidence.nodes = response.frame.nodes; evidence.camera = response.frame.camera; }
      if (response.camera) evidence.camera = response.camera;
      return packed;
    };
  }
  return result.instance ? { ...result, instance: { exports } } : { exports };
};
for (const type of [globalThis.WebGLRenderingContext, globalThis.WebGL2RenderingContext]) {
  if (!type) continue;
  const names = new WeakMap();
  const locate = type.prototype.getUniformLocation;
  type.prototype.getUniformLocation = function (program, name) {
    const location = locate.call(this, program, name);
    if (location) names.set(location, name);
    return location;
  };
  const uploadMatrix = type.prototype.uniformMatrix4fv;
  type.prototype.uniformMatrix4fv = function (location, transpose, values, ...rest) {
    if (names.get(location) === "projectionMatrix") evidence.projection = Array.from(values);
    return uploadMatrix.call(this, location, transpose, values, ...rest);
  };
  for (const name of ["bufferData", "bufferSubData"]) {
    const original = type.prototype[name];
    type.prototype[name] = function (...args) { evidence.uploads++; return original.apply(this, args); };
  }
}
