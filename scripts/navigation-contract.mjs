import assert from "node:assert/strict";

// Pure assertions shared by the real-WASM run and deliberately broken test samples.
export function checkNavigation(record, budget) {
  assert.equal(record.frames, budget.frames, "navigation sample coverage changed");
  assert.equal(record.preparations, 1, "camera navigation rebuilt scene geometry");
  assert.equal(record.cameraCalls, budget.frames, "navigation did extra camera work");
  assert.equal(record.fullFrameCalls, 0, "navigation used the legacy full-frame path");
  assert.equal(record.otherCalls, 0, "navigation round-tripped save/command/query state");
  assert.equal(record.liveAllocations, 0, "WASM input/output allocation leaked");
  assert.ok(record.maxInputBytes <= budget.maxInputBytes, "camera request grew");
  assert.ok(record.maxOutputBytes <= budget.maxOutputBytes, "camera response grew");
  assert.ok(record.inputBytes <= budget.inputBytes, "total camera input grew past baseline");
  assert.ok(record.outputBytes <= budget.outputBytes, "total camera output grew past baseline");
}

export function checkCoverage(records, expected) {
  const ids = records.map((record) => record.id);
  assert.equal(new Set(ids).size, ids.length, "duplicate benchmark fixture");
  assert.deepEqual([...ids].sort(), [...expected].sort(), "benchmark fixture missing or unexpected");
}

export function summarize(samples) {
  assert.ok(samples.length > 0 && samples.every((value) => Number.isFinite(value) && value >= 0));
  const sorted = [...samples].sort((a, b) => a - b);
  return {
    samples: samples.length,
    medianMs: sorted[Math.floor(sorted.length / 2)],
    p95Ms: sorted[Math.ceil(sorted.length * 0.95) - 1],
    minMs: sorted[0],
    maxMs: sorted.at(-1),
  };
}
