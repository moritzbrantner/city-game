import assert from "node:assert/strict";
import { checkDisplayedCamera } from "./navigation-contract.mjs";
import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { readFile, writeFile, mkdir, mkdtemp, rm, access } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve, extname, sep } from "node:path";

// Dependency-free CDP smoke against the built app, not a second UI implementation.
const dist = resolve(process.argv[2] ?? "web/dist");
const fixture = JSON.parse(await readFile(new URL("../fixtures/demo-scenario.json", import.meta.url)));
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const candidates = [process.env.CITY_GAME_CHROME, "/usr/bin/google-chrome", "/usr/bin/chromium", "/usr/bin/chromium-browser"].filter(Boolean);
let executable;
for (const candidate of candidates) {
  try { await access(candidate); executable = candidate; break; } catch { /* Try the next installed browser. */ }
}
assert.ok(executable, "Chrome/Chromium is required; set CITY_GAME_CHROME to its executable (never silently skip)");
assert.equal(typeof WebSocket, "function", "browser smoke requires Node 22+ with native WebSocket");
const scenarioManifest = { schemaVersion: 1, scenarios: ["one", "two", "three"].map((id) => ({
  id, name: `Navigation fixture ${id}`, region: "Deterministic regression scenario",
  description: "Known road, building, and water geometry for interaction regression tests.",
  dataKind: "Synthetic test fixture", scenario: "./test-scenario.json", frame: "./test-frame.json",
})) };
const server = createServer(async (request, response) => {
  try {
    const path = decodeURIComponent(new URL(request.url, "http://localhost").pathname);
    if (path === "/city-game/scenarios.json" || path === "/city-game/test-scenario.json") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify(path.endsWith("scenarios.json") ? scenarioManifest : fixture));
      return;
    }
    const file = resolve(dist, path.replace(/^\/city-game\//, "") || "index.html");
    if (!path.startsWith("/city-game/") || !file.startsWith(dist + sep)) throw new Error("outside test root");
    const mime = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".json": "application/json", ".wasm": "application/wasm" };
    response.writeHead(200, { "content-type": mime[extname(file)] ?? "application/octet-stream" });
    response.end(await readFile(file));
  } catch {
    if (!response.headersSent) response.writeHead(404);
    response.end();
  }
});
await new Promise((done) => server.listen(0, "127.0.0.1", done));
const profile = await mkdtemp(join(tmpdir(), "city-game-browser-"));
const chrome = spawn(executable, ["--headless=new", "--no-sandbox", "--disable-dev-shm-usage",
  "--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--remote-debugging-port=0",
  `--user-data-dir=${profile}`, "--window-size=1280,960", "about:blank"], { stdio: ["ignore", "ignore", "pipe"] });
let chromeError = null;
chrome.on("error", (error) => { chromeError = error; });
let chromeLog = "";
chrome.stderr.on("data", (data) => { chromeLog = (chromeLog + data).slice(-8000); });
let socket;
const pending = new Map();
const exceptions = [];
let sequence = 0;
const checks = [];
try {
  let port;
  for (let attempt = 0; attempt < 150; attempt++) {
    if (chromeError) throw chromeError;
    try { port = Number((await readFile(join(profile, "DevToolsActivePort"), "utf8")).split("\n")[0]); break; }
    catch { await sleep(100); }
  }
  assert.ok(port, `Chrome did not start: ${chromeLog}`);
  const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  const page = targets.find((target) => target.type === "page");
  assert.ok(page, "missing Chrome page target");
  socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((done, fail) => { socket.addEventListener("open", done); socket.addEventListener("error", fail); });
  socket.addEventListener("message", ({ data }) => {
    const message = JSON.parse(data);
    if (message.method === "Runtime.exceptionThrown") exceptions.push(message.params.exceptionDetails);
    const request = pending.get(message.id);
    if (!request) return;
    clearTimeout(request.timer); pending.delete(message.id);
    if (message.error) request.fail(new Error(JSON.stringify(message.error)));
    else request.done(message.result);
  });
  const send = (method, params = {}) => new Promise((done, fail) => {
    const id = ++sequence;
    const timer = setTimeout(() => { pending.delete(id); fail(new Error(`CDP timeout: ${method}`)); }, 15000);
    pending.set(id, { done, fail, timer });
    socket.send(JSON.stringify({ id, method, params }));
  });
  const evaluate = async (expression) => {
    const result = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    assert.equal(result.exceptionDetails, undefined, JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  const checkMenuLayout = async () => {
    const layout = await evaluate(`(() => {
      const menu = document.querySelector('.menu-shell');
      const button = document.querySelector('#choose-scenario').getBoundingClientRect();
      const panel = menu.getBoundingClientRect();
      return { overflow: menu.scrollWidth - menu.clientWidth,
        buttonFits: button.left >= panel.left && button.right <= panel.right };
    })()`);
    assert.ok(layout.overflow <= 1 && layout.buttonFits, `city menu overflow: ${JSON.stringify(layout)}`);
  };
  const settle = () => evaluate("new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))");
  const waitFor = async (expression) => {
    for (let attempt = 0; attempt < 150; attempt++) {
      if (await evaluate(expression)) return;
      assert.deepEqual(exceptions, [], "uncaught browser exception");
      await sleep(100);
    }
    throw new Error(`browser state timed out: ${expression}; ${JSON.stringify(await evaluate("({url:location.href,ready:document.readyState,body:document.body?.innerText?.slice(0,6000),calls:globalThis.__cityEvidence?.calls})"))}`);
  };
  await send("Runtime.enable"); await send("Page.enable"); await send("Network.enable");
  // Optional external settings/input distributions are explicitly unavailable in this offline
  // fixture lane. Core WASM, renderer, DOM controls and pointer handlers are real built code.
  await send("Network.setBlockedURLs", { urls: ["https://*", "http://*.github.io/*"] });
  await send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 960, deviceScaleFactor: 1, mobile: false });
  await send("Page.addScriptToEvaluateOnNewDocument", {
    source: await readFile(new URL("./browser-evidence-preload.js", import.meta.url), "utf8"),
  });
  const base = `http://127.0.0.1:${server.address().port}/city-game/`;
  await send("Page.navigate", { url: `${base}?scenario=one&mode=city` });
  await waitFor("document.body?.dataset.mode === 'city' && document.querySelector('#open-scenario')?.disabled === false && globalThis.__cityEvidence?.camera != null");
  await settle();
  await checkMenuLayout();
  const stats = () => evaluate("({...__cityEvidence.calls, uploads: __cityEvidence.uploads})");
  const initial = await stats();
  const originalCamera = await evaluate("__cityEvidence.camera");
  const originalProjection = await evaluate("__cityEvidence.projection");
  assert.ok(Array.isArray(originalProjection) && originalProjection.length === 16, "GPU projection was not observed");
  assert.equal(initial.city_game_session_prepare_render, 1);
  assert.ok(initial.uploads > 0, "the real WebGL renderer did not upload geometry");
  const click = async (selector) => { await evaluate(`document.querySelector(${JSON.stringify(selector)}).click()`); await settle(); };
  for (let index = 0; index < 8; index++) {
    await click("#view-zoom-in");
    const zoomed = await evaluate("__cityEvidence.projection");
    assert.notDeepEqual(zoomed, originalProjection, "zoom never reached the GPU");
    checkDisplayedCamera(zoomed, await evaluate("__cityEvidence.camera.projectionMatrix"));
    await click("#view-overview");
  }
  checks.push("zoom/displayed-camera-matches-Rust");
  assert.deepEqual(await evaluate("__cityEvidence.projection"), originalProjection, "displayed overview camera drifted");
  const beforeDuplicate = await stats();
  await click("#view-overview");
  assert.deepEqual(await stats(), beforeDuplicate, "duplicate overview did extra work");
  const afterNavigation = await stats();
  assert.equal(afterNavigation.city_game_session_prepare_render, 1);
  assert.equal(afterNavigation.uploads, initial.uploads, "camera-only navigation uploaded geometry");
  assert.equal(afterNavigation.city_game_render_frame_view ?? 0, 0);
  assert.equal(afterNavigation.city_game_render_frame ?? 0, 0);
  assert.equal(afterNavigation.city_game_session_execute ?? 0, 0);
  checks.push("zoom/overview/no-drift", "identical-overview/no-WASM-call", "camera-only/no-geometry-rebuild-or-GPU-upload");

  // Project the known building centre only to drive input. Simulation/render authority is untouched.
  const buildingPoint = await evaluate(`(() => {
    const camera = ${JSON.stringify(originalCamera)};
    const node = __cityEvidence.nodes.find(node => node.id === 'imported/way/20');
    const multiply = (m, p) => [0,1,2,3].map(row => p.reduce((sum,v,col) => sum + m[col*4+row]*v, 0));
    const view = multiply(camera.viewMatrix, [...node.transform.translation, 1]);
    const p = multiply(camera.projectionMatrix, view);
    const r = document.querySelector('#scene').getBoundingClientRect();
    return { x: r.left + (p[0]/p[3]+1)*r.width/2, y: r.top + (1-p[1]/p[3])*r.height/2 };
  })()`);
  const mouse = (type, point, extra = {}) => send("Input.dispatchMouseEvent", { type, ...point, ...extra });
  await mouse("mousePressed", buildingPoint, { button: "left", buttons: 1, clickCount: 1 });
  await mouse("mouseReleased", buildingPoint, { button: "left", buttons: 0, clickCount: 1 });
  await settle();
  assert.equal(await evaluate("document.querySelector('#selection-id').textContent"), "imported/way/20");
  await click("#view-focus");
  assert.equal(await evaluate("document.querySelector('#view-zoom').value"), "4.00×");
  checks.push("building-click/selection", "focus-selected");
  await mkdir(join(dist, "evidence"), { recursive: true });
  const screenshot = await send("Page.captureScreenshot", { format: "png" });
  await writeFile(join(dist, "evidence/navigation.png"), Buffer.from(screenshot.data, "base64"));
  await click("#view-overview");

  const rect = await evaluate("(() => {const r=document.querySelector('#scene').getBoundingClientRect();return {x:r.left+r.width/2,y:r.top+r.height/2};})()");
  let before = await stats();
  await mouse("mousePressed", rect, { button: "left", buttons: 1, clickCount: 1 });
  await mouse("mouseMoved", { x: rect.x + 50, y: rect.y + 20 }, { buttons: 1 });
  await mouse("mouseReleased", { x: rect.x + 50, y: rect.y + 20 }, { button: "left", buttons: 0, clickCount: 1 });
  await settle();
  assert.deepEqual(await stats(), before, "ordinary dragging panned or rebuilt the city");
  assert.equal(await evaluate("document.querySelector('#selection-id').textContent"), "imported/way/20");
  checks.push("unmodified-drag/no-pan-or-selection");

  await mouse("mousePressed", rect, { button: "left", buttons: 1, clickCount: 1 });
  before = await stats();
  await evaluate(`(() => {
    const canvas=document.querySelector('#scene'), id=__cityEvidence.pointerId;
    const r=canvas.getBoundingClientRect();
    canvas.dispatchEvent(new PointerEvent('pointermove',{pointerId:id,clientX:r.left+r.width/2+40,clientY:r.top+r.height/2,shiftKey:true}));
    canvas.dispatchEvent(new PointerEvent('pointercancel',{pointerId:id}));
  })()`);
  await mouse("mouseReleased", rect, { button: "left", buttons: 0, clickCount: 1 });
  await settle();
  assert.deepEqual(await stats(), before, "cancelled pointer applied queued movement");
  checks.push("pointercancel/discards-pending-pan");

  const projectionBeforePan = await evaluate("__cityEvidence.projection");
  await mouse("mousePressed", rect, { button: "left", buttons: 1, clickCount: 1 });
  before = await stats();
  await evaluate(`(() => {
    const canvas=document.querySelector('#scene'), id=__cityEvidence.pointerId;
    canvas.dispatchEvent(new PointerEvent('pointerup',{pointerId:id+100}));
    canvas.dispatchEvent(new PointerEvent('pointercancel',{pointerId:id+100}));
    const r=canvas.getBoundingClientRect();
    for(let i=1;i<=8;i++) canvas.dispatchEvent(new PointerEvent('pointermove',{
      pointerId:id,clientX:r.left+r.width/2+i*5,clientY:r.top+r.height/2,shiftKey:true}));
  })()`);
  await mouse("mouseReleased", { x: rect.x + 40, y: rect.y }, { button: "left", buttons: 0, clickCount: 1 });
  await settle();
  const afterDrag = await stats();
  const pannedProjection = await evaluate("__cityEvidence.projection");
  assert.notDeepEqual(pannedProjection, projectionBeforePan, "pan never reached the GPU");
  checkDisplayedCamera(pannedProjection, await evaluate("__cityEvidence.camera.projectionMatrix"));
  checks.push("pan/displayed-camera-matches-Rust");
  assert.equal(afterDrag.city_game_render_camera - before.city_game_render_camera, 1, "pan burst was not coalesced");
  assert.equal(afterDrag.city_game_session_prepare_render, before.city_game_session_prepare_render);
  assert.equal(afterDrag.uploads, before.uploads);
  checks.push("foreign-pointer/ignored", "shift-drag/coalesced-camera-update");

  before = await stats();
  await send("Emulation.setDeviceMetricsOverride", { width: 920, height: 720, deviceScaleFactor: 1, mobile: false });
  await settle();
  assert.equal((await stats()).city_game_session_prepare_render, before.city_game_session_prepare_render, "CSS resize rebuilt unchanged scene");
  await checkMenuLayout();
  await click("#choose-scenario");
  await click("#scenario-list button:nth-child(2)");
  await waitFor("document.querySelector('#scenario').textContent === 'Navigation fixture two' && !document.querySelector('#open-scenario').disabled");
  await click("#open-scenario");
  assert.equal(await evaluate("document.querySelector('#selection-name').textContent"), "Nothing");
  assert.equal(await evaluate("document.querySelector('#view-zoom').value"), "1.00×");
  assert.equal((await stats()).city_game_session_prepare_render, 2, "scenario switch did not prepare exactly one fresh scene");
  await checkMenuLayout();
  checks.push("resize/no-preparation", "scenario-switch/resets-selection-and-camera", "long-city-name/no-horizontal-menu-overflow");
  assert.deepEqual(exceptions, [], "uncaught browser exceptions");
  const evidence = { schemaVersion: 1, browser: await send("Browser.getVersion"), checks,
    stats: await stats(), fixture: "fixtures/demo-scenario.json", offlineOptionalFoundations: true,
    scope: "Built production JS/WASM/WebGL with deterministic fixture; optional remote settings/input modules deliberately blocked. Keyboard/wheel shared-foundation integration is not asserted by this lane." };
  await writeFile(join(dist, "evidence/browser-navigation.json"), JSON.stringify(evidence, null, 2) + "\n");
  console.log(`Browser navigation passed: ${checks.length} checks; screenshot evidence/navigation.png`);
} finally {
  for (const request of pending.values()) clearTimeout(request.timer);
  socket?.close();
  if (chrome.exitCode === null) {
    chrome.kill("SIGTERM");
    await Promise.race([new Promise((done) => chrome.once("exit", done)), sleep(3000)]);
    if (chrome.exitCode === null) chrome.kill("SIGKILL");
  }
  server.closeAllConnections();
  await new Promise((done) => server.close(done));
  await rm(profile, { recursive: true, force: true, maxRetries: 3 });
}
