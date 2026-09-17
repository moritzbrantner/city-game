const canvas = document.querySelector("#scene");
const sceneMode = document.querySelector("#scene-mode");
const sceneHint = document.querySelector("#scene-hint");
const menuContextLabel = document.querySelector("#menu-context-label");
const menuModeChip = document.querySelector("#menu-mode-chip");
const selectionName = document.querySelector("#selection-name");
const inspectionPanel = document.querySelector("#inspection-panel");
const inspectionSummary = document.querySelector("#inspection-summary");

if (!(canvas instanceof HTMLCanvasElement)) throw new Error("city-game surface is missing #scene");

const DRAG_THRESHOLD_PX = 4;
let pointerStart = null;
let shiftPressed = false;

function cityModeActive() {
  return document.body.dataset.mode === "city";
}

function syncModeSurface() {
  if (cityModeActive()) {
    menuContextLabel.textContent = "Explore the selected city";
    menuModeChip.textContent = "City";
    sceneMode.textContent = "Axonometric inspection";
    sceneHint.textContent =
      "Click to inspect · Shift + drag to move · wheel to zoom · Home for overview · F focuses selection";
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

canvas.addEventListener(
  "pointerdown",
  (event) => {
    if (!cityModeActive() || event.button !== 0) return;
    pointerStart = {
      pointerId: event.pointerId,
      clientX: event.clientX,
      clientY: event.clientY,
      cancelled: false,
    };
    shiftPressed = event.shiftKey;
    syncPanCursor();
  },
  true,
);

canvas.addEventListener(
  "pointermove",
  (event) => {
    if (!pointerStart || pointerStart.pointerId !== event.pointerId || !cityModeActive()) return;
    shiftPressed = event.shiftKey;
    syncPanCursor();
    if (event.shiftKey || pointerStart.cancelled) return;
    const distance = Math.hypot(event.clientX - pointerStart.clientX, event.clientY - pointerStart.clientY);
    if (distance < DRAG_THRESHOLD_PX) return;

    pointerStart.cancelled = true;
    canvas.dispatchEvent(
      new PointerEvent("pointercancel", {
        bubbles: true,
        pointerId: event.pointerId,
        pointerType: event.pointerType,
        isPrimary: event.isPrimary,
      }),
    );
    event.stopImmediatePropagation();
  },
  true,
);

for (const eventName of ["pointerup", "pointercancel"]) {
  canvas.addEventListener(
    eventName,
    (event) => {
      if (!pointerStart || pointerStart.pointerId !== event.pointerId) return;
      pointerStart = null;
      shiftPressed = event.shiftKey;
      syncPanCursor();
    },
    true,
  );
}

syncModeSurface();
syncSelectionSurface();
