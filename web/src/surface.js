const canvas = document.querySelector("#scene");
const menuContextLabel = document.querySelector("#menu-context-label");
const menuModeChip = document.querySelector("#menu-mode-chip");
const selectionName = document.querySelector("#selection-name");
const inspectionPanel = document.querySelector("#inspection-panel");
const inspectionSummary = document.querySelector("#inspection-summary");

if (!(canvas instanceof HTMLCanvasElement)) throw new Error("city-game surface is missing #scene");

let shiftPressed = false;

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
  syncPanCursor();
}

function syncSelectionSurface() {
  if (!(selectionName instanceof HTMLElement) || !(inspectionSummary instanceof HTMLElement)) return;
  const label = selectionName.textContent?.trim() || "Nothing";
  const selected = label !== "Nothing";
  inspectionSummary.textContent = selected ? label : "Nothing selected";
  if (selected && inspectionPanel instanceof HTMLDetailsElement) inspectionPanel.open = true;
}

function syncPanCursor() {
  if (cityModeActive() && shiftPressed) canvas.dataset.panModifier = "true";
  else delete canvas.dataset.panModifier;
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
