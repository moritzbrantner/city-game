const canvas = document.querySelector("#scene");
const menuContextLabel = document.querySelector("#menu-context-label");
const menuModeChip = document.querySelector("#menu-mode-chip");
const selectionName = document.querySelector("#selection-name");
const inspectionPanel = document.querySelector("#inspection-panel");
const inspectionSummary = document.querySelector("#inspection-summary");
const sceneModeHeading = document.querySelector("#scene-mode");
const scenarioList = document.querySelector("#scenario-list");

if (!(canvas instanceof HTMLCanvasElement)) throw new Error("city-game surface is missing #scene");

let shiftPressed = false;
const preparedScenarioButtons = new WeakSet();
const prefetchedScenarioIds = new Set();
const scenarioManifestPromise = fetch("./scenarios.json", { cache: "force-cache" })
  .then((response) => {
    if (!response.ok) throw new Error(`failed to load scenario manifest for prefetch: ${response.status}`);
    return response.json();
  })
  .catch(() => null);

function cityModeActive() {
  return document.body.dataset.mode === "city";
}

function syncModeSurface() {
  if (cityModeActive()) {
    menuContextLabel.textContent = "Explore the selected city";
    menuModeChip.textContent = "City";
  } else {
    menuContextLabel.textContent = "Choose a city";
    menuModeChip.textContent = "Scenario";
  }
  syncSelectionSurface();
  syncPanCursor();
}

function syncSelectionSurface() {
  if (!(selectionName instanceof HTMLElement) || !(inspectionSummary instanceof HTMLElement)) return;
  const label = selectionName.textContent?.trim() || "Nothing";
  const selected = cityModeActive() && label !== "Nothing";
  document.body.dataset.selection = selected ? "active" : "none";
  inspectionSummary.textContent = selected ? label : "Nothing selected";
  if (selected && inspectionPanel instanceof HTMLDetailsElement) inspectionPanel.open = true;
  if (sceneModeHeading instanceof HTMLElement && cityModeActive()) {
    sceneModeHeading.textContent = selected ? `Selected · ${label}` : "Isometric inspection";
  }
}

function syncPanCursor() {
  if (cityModeActive() && shiftPressed) canvas.dataset.panModifier = "true";
  else delete canvas.dataset.panModifier;
}

async function prepareScenarioButtons() {
  if (!(scenarioList instanceof HTMLElement)) return;
  const manifest = await scenarioManifestPromise;
  if (!manifest || !Array.isArray(manifest.scenarios)) return;
  const scenariosByName = new Map(manifest.scenarios.map((scenario) => [scenario.name, scenario]));

  for (const button of scenarioList.querySelectorAll(".scenario-option")) {
    if (!(button instanceof HTMLButtonElement) || preparedScenarioButtons.has(button)) continue;
    const name = button.querySelector("strong")?.textContent?.trim();
    const scenario = scenariosByName.get(name);
    if (!scenario || typeof scenario.id !== "string" || typeof scenario.scenario !== "string") continue;

    preparedScenarioButtons.add(button);
    button.dataset.scenarioId = scenario.id;
    button.dataset.prefetch = "ready";
    button.title = `Preview ${scenario.name}; opening it uses its own scenario page`;
    const prefetch = () => prefetchScenario(scenario);
    button.addEventListener("pointerenter", prefetch, { once: true, passive: true });
    button.addEventListener("focus", prefetch, { once: true, passive: true });
  }
}

function prefetchScenario(scenario) {
  if (prefetchedScenarioIds.has(scenario.id)) return;
  prefetchedScenarioIds.add(scenario.id);
  fetch(scenario.scenario, { cache: "force-cache" }).catch(() => {
    prefetchedScenarioIds.delete(scenario.id);
  });
}

new MutationObserver(syncModeSurface).observe(document.body, {
  attributes: true,
  attributeFilter: ["data-mode"],
});
if (selectionName instanceof HTMLElement) {
  new MutationObserver(syncSelectionSurface).observe(selectionName, {
    childList: true,
    characterData: true,
    subtree: true,
  });
}
if (scenarioList instanceof HTMLElement) {
  new MutationObserver(() => void prepareScenarioButtons()).observe(scenarioList, { childList: true });
}

window.addEventListener("keydown", (event) => {
  if (event.key !== "Shift") return;
  shiftPressed = true;
  syncPanCursor();
});
window.addEventListener("keyup", (event) => {
  if (event.key !== "Shift") return;
  shiftPressed = false;
  syncPanCursor();
});
window.addEventListener("blur", () => {
  shiftPressed = false;
  syncPanCursor();
});

syncModeSurface();
syncSelectionSurface();
void prepareScenarioButtons();
