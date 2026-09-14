import { createThreeSceneRenderer, validateRenderFrame } from "@moritzbrantner/three-d-renderer";

const canvas = document.querySelector("#scene");
const scenarioList = document.querySelector("#scenario-list");
const scenarioLabel = document.querySelector("#scenario");
const regionLabel = document.querySelector("#region");
const objectsLabel = document.querySelector("#objects");
const dataKindLabel = document.querySelector("#data-kind");
const descriptionLabel = document.querySelector("#scenario-description");
const statusLabel = document.querySelector("#status");

if (!(canvas instanceof HTMLCanvasElement) || !(scenarioList instanceof HTMLElement)) {
  throw new Error("city-game scenario surface is incomplete");
}

const manifestResponse = await fetch("./scenarios.json");
if (!manifestResponse.ok) {
  throw new Error(`failed to load scenario manifest: ${manifestResponse.status}`);
}
const manifest = await manifestResponse.json();
if (manifest?.schemaVersion !== 1 || !Array.isArray(manifest.scenarios) || manifest.scenarios.length < 3) {
  throw new Error("scenario manifest must contain at least three versioned examples");
}

const scenarios = new Map();
for (const scenario of manifest.scenarios) {
  if (
    typeof scenario?.id !== "string" ||
    typeof scenario?.name !== "string" ||
    typeof scenario?.region !== "string" ||
    typeof scenario?.description !== "string" ||
    typeof scenario?.dataKind !== "string" ||
    typeof scenario?.frame !== "string" ||
    scenarios.has(scenario.id)
  ) {
    throw new Error("scenario manifest contains an invalid or duplicate entry");
  }
  scenarios.set(scenario.id, scenario);
}

const renderer = createThreeSceneRenderer(canvas, {
  antialias: true,
  background: 0xe5e4de,
  pixelRatioLimit: 2,
  shadows: false,
});
let currentFrame = null;
let loadGeneration = 0;
const buttons = new Map();

function render() {
  if (!currentFrame) return;
  const width = Math.max(1, canvas.clientWidth);
  const height = Math.max(1, canvas.clientHeight);
  renderer.setSize(width, height, window.devicePixelRatio);
  renderer.render(currentFrame);
}

async function selectScenario(scenario, updateUrl = true) {
  const generation = ++loadGeneration;
  statusLabel.textContent = `Loading ${scenario.name}…`;
  const response = await fetch(scenario.frame);
  if (!response.ok) {
    throw new Error(`failed to load ${scenario.name}: ${response.status}`);
  }
  const frame = validateRenderFrame(await response.json());
  if (!Number.isFinite(frame.camera.aspect) || frame.camera.aspect <= 0) {
    throw new Error(`${scenario.name} frame must declare a finite positive camera aspect`);
  }
  if (generation !== loadGeneration) return;

  currentFrame = frame;
  canvas.style.aspectRatio = String(frame.camera.aspect);
  scenarioLabel.textContent = scenario.name;
  regionLabel.textContent = scenario.region;
  objectsLabel.textContent = String(frame.nodes.length);
  dataKindLabel.textContent = scenario.dataKind;
  descriptionLabel.textContent = scenario.description;
  statusLabel.textContent = "";
  for (const [id, button] of buttons) {
    button.setAttribute("aria-pressed", String(id === scenario.id));
  }
  if (updateUrl) {
    const url = new URL(window.location.href);
    url.searchParams.set("scenario", scenario.id);
    history.replaceState(null, "", url);
  }
  render();
}

for (const scenario of scenarios.values()) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "scenario-option";
  button.setAttribute("aria-pressed", "false");
  const name = document.createElement("strong");
  name.textContent = scenario.name;
  const region = document.createElement("span");
  region.textContent = scenario.region;
  button.append(name, region);
  button.addEventListener("click", () => {
    selectScenario(scenario).catch((error) => {
      statusLabel.textContent = error instanceof Error ? error.message : String(error);
    });
  });
  buttons.set(scenario.id, button);
  scenarioList.append(button);
}

const requestedId = new URL(window.location.href).searchParams.get("scenario");
const initialScenario = scenarios.get(requestedId) ?? scenarios.values().next().value;
await selectScenario(initialScenario);

const observer = new ResizeObserver(render);
observer.observe(canvas);
window.addEventListener("pagehide", () => {
  observer.disconnect();
  renderer.dispose();
});
