import {
  createThreeSceneRenderer,
  projectWorldPoint,
  validateRenderFrame,
} from "@moritzbrantner/three-d-renderer";
import { createCityGameRuntime } from "./wasm.js";

const DEFAULT_FRAME_ASPECT = 16 / 9;
const SETTINGS_STORAGE_KEY = "city-game.settings.user.v1";
const MIN_ZOOM = 0.5;
const MAX_ZOOM = 32;
const MAX_PAN = 8;
const ZOOM_FACTOR = 1.25;
const KEYBOARD_PAN_STEP = 0.08;
const DRAG_THRESHOLD_PX = 4;
const OVERVIEW_VIEW = Object.freeze({ panX: 0, panY: 0, zoom: 1 });

const APPEARANCE_DEFINITIONS = [
  {
    id: "appearance.color_scheme",
    kind: { type: "choice", options: ["system", "light", "dark"] },
    default: { type: "choice", value: "system" },
    scope: "user",
    apply_mode: "immediate",
  },
  {
    id: "appearance.contrast",
    kind: { type: "choice", options: ["system", "normal", "high", "low"] },
    default: { type: "choice", value: "system" },
    scope: "user",
    apply_mode: "immediate",
  },
  {
    id: "appearance.color_vision",
    kind: {
      type: "choice",
      options: ["off", "protanopia", "deuteranopia", "tritanopia", "achromatopsia"],
    },
    default: { type: "choice", value: "off" },
    scope: "user",
    apply_mode: "immediate",
  },
  {
    id: "appearance.night_mode",
    kind: { type: "bool" },
    default: { type: "bool", value: false },
    scope: "user",
    apply_mode: "immediate",
  },
];

const settingsModulePromise = import("@moritzbrantner/settings-browser").catch((error) => ({
  foundationLoadError: error,
}));
const inputBindingsModulePromise = import("@moritzbrantner/input-bindings-browser").catch((error) => ({
  foundationLoadError: error,
}));

const canvas = requiredElement("#scene", HTMLCanvasElement);
const scenarioList = requiredElement("#scenario-list", HTMLElement);
const scenarioPanel = requiredElement("#scenario-panel", HTMLElement);
const cityPanel = requiredElement("#city-panel", HTMLElement);
const scenarioLabel = requiredElement("#scenario", HTMLElement);
const regionLabel = requiredElement("#region", HTMLElement);
const objectsLabel = requiredElement("#objects", HTMLElement);
const dataKindLabel = requiredElement("#data-kind", HTMLElement);
const descriptionLabel = requiredElement("#scenario-description", HTMLElement);
const openScenarioButton = requiredElement("#open-scenario", HTMLButtonElement);
const chooseScenarioButton = requiredElement("#choose-scenario", HTMLButtonElement);
const cityTitle = requiredElement("#city-title", HTMLElement);
const cityRegion = requiredElement("#city-region", HTMLElement);
const overviewRoads = requiredElement("#overview-roads", HTMLElement);
const overviewBuildings = requiredElement("#overview-buildings", HTMLElement);
const overviewWater = requiredElement("#overview-water", HTMLElement);
const overviewLandUse = requiredElement("#overview-land-use", HTMLElement);
const selectionName = requiredElement("#selection-name", HTMLElement);
const selectionKind = requiredElement("#selection-kind", HTMLElement);
const selectionId = requiredElement("#selection-id", HTMLElement);
const selectionExtra = requiredElement("#selection-extra", HTMLElement);
const sceneModeLabel = requiredElement("#scene-mode", HTMLElement);
const sceneHintLabel = requiredElement("#scene-hint", HTMLElement);
const viewOverviewButton = requiredElement("#view-overview", HTMLButtonElement);
const viewZoomOutButton = requiredElement("#view-zoom-out", HTMLButtonElement);
const viewZoomInButton = requiredElement("#view-zoom-in", HTMLButtonElement);
const viewFocusButton = requiredElement("#view-focus", HTMLButtonElement);
const viewZoomLabel = requiredElement("#view-zoom", HTMLOutputElement);
const settingsStatus = requiredElement("#settings-status", HTMLElement);
const inputStatus = requiredElement("#input-status", HTMLElement);
const statusLabel = requiredElement("#status", HTMLElement);
const colorSchemeControl = requiredElement("#setting-color-scheme", HTMLSelectElement);
const contrastControl = requiredElement("#setting-contrast", HTMLSelectElement);
const colorVisionControl = requiredElement("#setting-color-vision", HTMLSelectElement);
const nightModeControl = requiredElement("#setting-night-mode", HTMLInputElement);

const systemColorScheme = window.matchMedia("(prefers-color-scheme: dark)");
const systemHighContrast = window.matchMedia("(prefers-contrast: more)");
const systemLowContrast = window.matchMedia("(prefers-contrast: less)");

let resolvedAppearance = {
  colorScheme: systemColorScheme.matches ? "dark" : "light",
  contrast: resolveSystemContrast(),
  colorVision: "off",
  nightMode: false,
};
applyAppearanceDataset();

const [manifestResponse, runtime] = await Promise.all([
  fetch("./scenarios.json"),
  createCityGameRuntime("./city-game-core.wasm"),
]);
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
    typeof scenario?.scenario !== "string" ||
    typeof scenario?.frame !== "string" ||
    scenarios.has(scenario.id)
  ) {
    throw new Error("scenario manifest contains an invalid or duplicate entry");
  }
  scenarios.set(scenario.id, scenario);
}
const scenarioOrder = [...scenarios.values()];

let renderer = createRenderer();
let appMode = "picker";
let currentFrame = null;
let currentSession = null;
let currentScenario = null;
let currentCanonicalScenario = null;
let entityIndex = new Map();
let selectedEntity = null;
let cameraView = { ...OVERVIEW_VIEW };
let loadGeneration = 0;
let lastCanvasPointer = null;
let pointerGesture = null;
let pendingPan = { x: 0, y: 0 };
let panAnimationFrame = null;
let settingsSession = null;
let inputController = null;
let detachKeyboard = null;
let detachMouse = null;
const buttons = new Map();

function createRenderer() {
  return createThreeSceneRenderer(canvas, {
    antialias: true,
    background: rendererBackground(),
    pixelRatioLimit: 2,
    shadows: false,
  });
}

function rendererBackground() {
  if (resolvedAppearance.colorScheme === "dark") {
    if (resolvedAppearance.contrast === "high") return 0x080a08;
    return resolvedAppearance.nightMode ? 0x11130f : 0x191c18;
  }
  if (resolvedAppearance.contrast === "high") return 0xf7f7f2;
  return resolvedAppearance.nightMode ? 0xe4dccd : 0xe5e4de;
}

function recreateRenderer() {
  const previous = renderer;
  renderer = createRenderer();
  previous?.dispose();
  render();
}

function render() {
  if (!currentFrame) return;
  const width = Math.max(1, canvas.clientWidth);
  const height = Math.max(1, canvas.clientHeight);
  renderer.setSize(width, height, window.devicePixelRatio);
  renderer.render(presentedFrame());
}

function presentedFrame() {
  if (!currentFrame || !selectedEntity) return currentFrame;

  const accent = selectionColor();
  const nodes = [];
  for (const node of currentFrame.nodes) {
    if (entityIdForNode(node.id) !== selectedEntity.id) {
      nodes.push(node);
      continue;
    }

    nodes.push({ ...node, color: accent, opacity: 1 });
    if (node.transform) {
      const scale = node.transform.scale ?? [1, 1, 1];
      nodes.push({
        ...node,
        id: `selection-outline/${node.id}`,
        color: accent,
        opacity: 1,
        wireframe: true,
        transform: {
          ...node.transform,
          scale: scale.map((value) => value * 1.06),
        },
      });
    }
  }
  return { ...currentFrame, nodes };
}

function selectionColor() {
  switch (resolvedAppearance.colorVision) {
    case "protanopia":
    case "deuteranopia":
      return 0x0072b2;
    case "tritanopia":
      return 0xcc79a7;
    case "achromatopsia":
      return resolvedAppearance.colorScheme === "dark" ? 0xffffff : 0x111111;
    default:
      return 0xd17f00;
  }
}

async function selectScenario(scenario, updateUrl = true) {
  const generation = ++loadGeneration;
  openScenarioButton.disabled = true;
  statusLabel.textContent = `Loading ${scenario.name} through Rust/WASM…`;

  const response = await fetch(scenario.scenario);
  if (!response.ok) {
    throw new Error(`failed to load ${scenario.name} scenario: ${response.status}`);
  }
  const canonicalScenario = await response.json();
  const session = runtime.createSession(canonicalScenario);
  const nextView = { ...OVERVIEW_VIEW };
  const frame = validateRenderFrame(session.renderFrame(DEFAULT_FRAME_ASPECT, nextView));
  if (!Number.isFinite(frame.camera.aspect) || frame.camera.aspect <= 0) {
    throw new Error(`${scenario.name} frame must declare a finite positive camera aspect`);
  }
  if (generation !== loadGeneration) return;

  currentScenario = scenario;
  currentCanonicalScenario = canonicalScenario;
  currentSession = session;
  currentFrame = frame;
  cameraView = nextView;
  entityIndex = buildEntityIndex(canonicalScenario);
  setSelectedEntity(null);
  updateOverview(canonicalScenario);
  updateViewControls();
  canvas.style.aspectRatio = String(frame.camera.aspect);
  scenarioLabel.textContent = scenario.name;
  regionLabel.textContent = scenario.region;
  objectsLabel.textContent = String(frame.nodes.length);
  dataKindLabel.textContent = scenario.dataKind;
  descriptionLabel.textContent = scenario.description;
  cityTitle.textContent = scenario.name;
  cityRegion.textContent = scenario.region;
  openScenarioButton.textContent = `Open ${scenario.name}`;
  openScenarioButton.disabled = false;
  statusLabel.textContent = "Scenario ready. Open it to inspect the city.";
  for (const [id, button] of buttons) {
    button.setAttribute("aria-pressed", String(id === scenario.id));
  }
  if (updateUrl) updateLocation();
  render();
}

function updateOverview(scenario) {
  overviewRoads.textContent = String(scenario.roads?.length ?? 0);
  overviewBuildings.textContent = String(scenario.buildings?.length ?? 0);
  overviewWater.textContent = String(scenario.water?.length ?? 0);
  overviewLandUse.textContent = String(scenario.landUseAreas?.length ?? 0);
}

function setMode(mode, updateUrl = true) {
  if (mode !== "picker" && mode !== "city") throw new Error(`unsupported city-game mode: ${mode}`);
  if (mode === "city" && !currentScenario) return;

  appMode = mode;
  document.body.dataset.mode = mode;
  scenarioPanel.hidden = mode !== "picker";
  cityPanel.hidden = mode !== "city";
  if (mode === "picker") {
    setSelectedEntity(null);
    resetView(false);
    sceneModeLabel.textContent = "Scenario preview";
    sceneHintLabel.textContent = "Choose a city, then open it to inspect and navigate.";
    canvas.setAttribute("aria-label", "Scenario preview");
    statusLabel.textContent = "Choose a scenario to preview or open.";
  } else {
    sceneModeLabel.textContent = "Isometric inspection";
    sceneHintLabel.textContent = "Click to inspect · Shift + drag to move · wheel to zoom · Home for overview · F focuses selection";
    canvas.setAttribute("aria-label", `Inspect and navigate ${currentScenario.name}`);
    statusLabel.textContent = "Inspection mode active. City state remains unchanged.";
    canvas.focus({ preventScroll: true });
  }
  if (updateUrl) updateLocation();
  render();
}

function updateLocation() {
  if (!currentScenario) return;
  const url = new URL(window.location.href);
  url.searchParams.set("scenario", currentScenario.id);
  if (appMode === "city") url.searchParams.set("mode", "city");
  else url.searchParams.delete("mode");
  history.replaceState(null, "", url);
}

async function selectScenarioByOffset(offset) {
  if (!currentScenario || scenarioOrder.length === 0) return;
  const currentIndex = scenarioOrder.findIndex((scenario) => scenario.id === currentScenario.id);
  const nextIndex = (currentIndex + offset + scenarioOrder.length) % scenarioOrder.length;
  await selectScenario(scenarioOrder[nextIndex]);
}

function buildEntityIndex(scenario) {
  const index = new Map();
  addEntities(index, scenario.roads, "Road");
  addEntities(index, scenario.buildings, "Building");
  addEntities(index, scenario.water, "Water");
  addEntities(index, scenario.landUseAreas, "Land use");
  return index;
}

function addEntities(index, entities, kind) {
  if (!Array.isArray(entities)) return;
  for (const entity of entities) {
    if (typeof entity?.id === "string" && !index.has(entity.id)) {
      index.set(entity.id, { id: entity.id, kind, entity });
    }
  }
}

function entityIdForNode(nodeId) {
  const roadSegment = /^(.*)\/road-segment-\d+$/.exec(nodeId);
  return roadSegment ? roadSegment[1] : nodeId;
}

function setSelectedEntity(selection) {
  selectedEntity = selection;
  viewFocusButton.disabled = !selection;
  selectionExtra.replaceChildren();
  if (!selection) {
    selectionName.textContent = "Nothing";
    selectionKind.textContent = "—";
    selectionId.textContent = "—";
  } else {
    selectionName.textContent = entityDisplayName(selection);
    selectionKind.textContent = selection.kind;
    selectionId.textContent = selection.id;
    renderSelectionDetails(selection);
  }
  render();
}

function renderSelectionDetails(selection) {
  const entity = selection.entity ?? {};
  const rows = [];
  if (selection.kind === "Road") {
    rows.push(["Class", titleCase(entity.class ?? "unknown")]);
    rows.push(["Lanes", entity.lanes ?? "—"]);
    rows.push(["Speed", Number.isFinite(entity.maxSpeedKph) ? `${entity.maxSpeedKph} km/h` : "—"]);
  } else if (selection.kind === "Building") {
    rows.push(["Use", titleCase(entity.useKind ?? "unknown")]);
    rows.push(["Levels", entity.levels ?? "—"]);
    rows.push(["Height", Number.isFinite(entity.heightM) ? `${entity.heightM} m` : "derived from levels"]);
    rows.push([
      "Floor area",
      Number.isFinite(entity.grossFloorAreaM2) ? `${formatNumber(entity.grossFloorAreaM2)} m²` : "—",
    ]);
  } else if (selection.kind === "Land use") {
    rows.push(["Use", titleCase(entity.kind ?? "unknown")]);
  } else if (selection.kind === "Water") {
    rows.push(["Kind", titleCase(entity.kind ?? "water")]);
  }
  const geometry = entity.geometry ?? entity.footprint;
  if (geometry?.type) rows.push(["Geometry", geometry.type]);
  if (entity.sourceId) rows.push(["Source", entity.sourceId, true]);

  for (const [label, value, monospace = false] of rows) {
    const row = document.createElement("div");
    const term = document.createElement("dt");
    const detail = document.createElement("dd");
    term.textContent = label;
    detail.textContent = String(value);
    if (monospace) detail.dataset.monospace = "true";
    row.append(term, detail);
    selectionExtra.append(row);
  }
}

function entityDisplayName(selection) {
  const entity = selection.entity ?? {};
  if (typeof entity.name === "string" && entity.name.length > 0) return entity.name;
  if (selection.kind === "Building" && typeof entity.useKind === "string") {
    return `${titleCase(entity.useKind)} building`;
  }
  if (selection.kind === "Land use" && typeof entity.kind === "string") {
    return `${titleCase(entity.kind)} area`;
  }
  return selection.id;
}

function titleCase(value) {
  const text = String(value ?? "");
  return text.length === 0 ? text : text[0].toUpperCase() + text.slice(1).replaceAll("_", " ");
}

function formatNumber(value) {
  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(value);
}

function refreshFrame() {
  if (!currentSession) return;
  const frame = validateRenderFrame(currentSession.renderFrame(DEFAULT_FRAME_ASPECT, cameraView));
  if (!Number.isFinite(frame.camera.aspect) || frame.camera.aspect <= 0) {
    throw new Error("inspection frame must declare a finite positive camera aspect");
  }
  currentFrame = frame;
  updateViewControls();
  render();
}

function setCameraView(nextView, status) {
  if (!currentSession) return;
  cameraView = {
    panX: clamp(Number(nextView.panX), -MAX_PAN, MAX_PAN),
    panY: clamp(Number(nextView.panY), -MAX_PAN, MAX_PAN),
    zoom: clamp(Number(nextView.zoom), MIN_ZOOM, MAX_ZOOM),
  };
  refreshFrame();
  if (status) statusLabel.textContent = status;
}

function resetView(announce = true) {
  setCameraView(
    OVERVIEW_VIEW,
    announce ? "Overview restored. Inspection remains read-only." : undefined,
  );
}

function zoomBy(factor, announce = true) {
  const nextZoom = clamp(cameraView.zoom * factor, MIN_ZOOM, MAX_ZOOM);
  setCameraView(
    { ...cameraView, zoom: nextZoom },
    announce ? `Zoom ${nextZoom.toFixed(2)}×` : undefined,
  );
}

function panBy(deltaX, deltaY, announce = false) {
  setCameraView(
    {
      ...cameraView,
      panX: cameraView.panX + deltaX,
      panY: cameraView.panY + deltaY,
    },
    announce ? "View moved." : undefined,
  );
}

function updateViewControls() {
  viewZoomLabel.value = `${cameraView.zoom.toFixed(2)}×`;
  viewZoomLabel.textContent = viewZoomLabel.value;
  viewZoomOutButton.disabled = cameraView.zoom <= MIN_ZOOM + Number.EPSILON;
  viewZoomInButton.disabled = cameraView.zoom >= MAX_ZOOM - Number.EPSILON;
  viewFocusButton.disabled = !selectedEntity;
}

function focusSelection() {
  if (!selectedEntity || !currentFrame) return;
  const rect = canvas.getBoundingClientRect();
  const bounds = selectedScreenBounds(rect.width, rect.height);
  if (!bounds) return;

  const centerX = (bounds.minX + bounds.maxX) * 0.5;
  const centerY = (bounds.minY + bounds.maxY) * 0.5;
  const nextPanX = cameraView.panX + (centerX / rect.width - 0.5) / cameraView.zoom;
  const nextPanY = cameraView.panY + (0.5 - centerY / rect.height) / cameraView.zoom;
  const nextZoom = Math.max(cameraView.zoom, 4);
  setCameraView(
    { panX: nextPanX, panY: nextPanY, zoom: nextZoom },
    `Focused ${entityDisplayName(selectedEntity)} at ${nextZoom.toFixed(2)}×.`,
  );
}

function selectedScreenBounds(width, height) {
  const matches = currentFrame.nodes.filter((node) => entityIdForNode(node.id) === selectedEntity.id);
  const bounds = matches
    .map((node) => projectNodeBounds(currentFrame.camera, node, width, height))
    .filter(Boolean);
  if (bounds.length === 0) return null;
  return {
    minX: Math.min(...bounds.map((entry) => entry.minX)),
    maxX: Math.max(...bounds.map((entry) => entry.maxX)),
    minY: Math.min(...bounds.map((entry) => entry.minY)),
    maxY: Math.max(...bounds.map((entry) => entry.maxY)),
  };
}

function pickAtPointer(pointer) {
  if (appMode !== "city" || !currentFrame || !pointer) return;
  const node = pickNode(currentFrame, pointer);
  if (!node) {
    setSelectedEntity(null);
    statusLabel.textContent = "Selection cleared.";
    return;
  }
  const entityId = entityIdForNode(node.id);
  const selection = entityIndex.get(entityId) ?? { id: entityId, kind: "Object", entity: {} };
  setSelectedEntity(selection);
  statusLabel.textContent = `Inspecting ${entityDisplayName(selection)}.`;
}

function pickNode(frame, pointer) {
  const candidates = [];
  for (const node of frame.nodes) {
    const bounds = projectNodeBounds(frame.camera, node, pointer.width, pointer.height);
    if (!bounds) continue;
    const slop = 8;
    if (
      pointer.x < bounds.minX - slop ||
      pointer.x > bounds.maxX + slop ||
      pointer.y < bounds.minY - slop ||
      pointer.y > bounds.maxY + slop
    ) {
      continue;
    }
    const centerX = (bounds.minX + bounds.maxX) * 0.5;
    const centerY = (bounds.minY + bounds.maxY) * 0.5;
    candidates.push({
      node,
      depth: bounds.depth,
      distanceSquared: (pointer.x - centerX) ** 2 + (pointer.y - centerY) ** 2,
      area: Math.max(1, (bounds.maxX - bounds.minX) * (bounds.maxY - bounds.minY)),
    });
  }
  candidates.sort(
    (left, right) =>
      left.depth - right.depth ||
      left.distanceSquared - right.distanceSquared ||
      left.area - right.area ||
      left.node.id.localeCompare(right.node.id),
  );
  return candidates[0]?.node ?? null;
}

function projectNodeBounds(camera, node, width, height) {
  if (node.geometry?.kind !== "box") return null;
  const [sizeX, sizeY, sizeZ] = node.geometry.size;
  const projected = [];
  for (const x of [-sizeX * 0.5, sizeX * 0.5]) {
    for (const y of [-sizeY * 0.5, sizeY * 0.5]) {
      for (const z of [-sizeZ * 0.5, sizeZ * 0.5]) {
        const worldPoint = transformNodePoint(node, [x, y, z]);
        const point = projectWorldPoint(camera, worldPoint, { width, height });
        if (point.visible) projected.push(point);
      }
    }
  }
  if (projected.length === 0) return null;
  return {
    minX: Math.min(...projected.map((point) => point.x)),
    maxX: Math.max(...projected.map((point) => point.x)),
    minY: Math.min(...projected.map((point) => point.y)),
    maxY: Math.max(...projected.map((point) => point.y)),
    depth: Math.min(...projected.map((point) => point.depth)),
  };
}

function transformNodePoint(node, point) {
  if (Array.isArray(node.modelMatrix)) return transformMatrixPoint(node.modelMatrix, point);
  const transform = node.transform;
  const scale = transform.scale ?? [1, 1, 1];
  const scaled = point.map((value, index) => value * scale[index]);
  const rotated = rotateQuaternion(
    scaled,
    normalizeQuaternion(transform.rotationQuaternion ?? [0, 0, 0, 1]),
  );
  return rotated.map((value, index) => value + transform.translation[index]);
}

function transformMatrixPoint(matrix, [x, y, z]) {
  const worldX = matrix[0] * x + matrix[4] * y + matrix[8] * z + matrix[12];
  const worldY = matrix[1] * x + matrix[5] * y + matrix[9] * z + matrix[13];
  const worldZ = matrix[2] * x + matrix[6] * y + matrix[10] * z + matrix[14];
  const worldW = matrix[3] * x + matrix[7] * y + matrix[11] * z + matrix[15];
  if (!Number.isFinite(worldW) || Math.abs(worldW) <= Number.EPSILON) return [worldX, worldY, worldZ];
  return [worldX / worldW, worldY / worldW, worldZ / worldW];
}

function normalizeQuaternion(quaternion) {
  const length = Math.hypot(...quaternion);
  if (!Number.isFinite(length) || length <= Number.EPSILON) return [0, 0, 0, 1];
  return quaternion.map((value) => value / length);
}

function rotateQuaternion([x, y, z], [qx, qy, qz, qw]) {
  const ix = qw * x + qy * z - qz * y;
  const iy = qw * y + qz * x - qx * z;
  const iz = qw * z + qx * y - qy * x;
  const iw = -qx * x - qy * y - qz * z;
  return [
    ix * qw + iw * -qx + iy * -qz - iz * -qy,
    iy * qw + iw * -qy + iz * -qx - ix * -qz,
    iz * qw + iw * -qz + ix * -qy - iy * -qx,
  ];
}

function clamp(value, minimum, maximum) {
  if (!Number.isFinite(value)) return minimum;
  return Math.min(maximum, Math.max(minimum, value));
}

async function initializeSettings() {
  const module = await settingsModulePromise;
  if (module.foundationLoadError) throw module.foundationLoadError;
  if (typeof module.createSettingsSession !== "function") {
    throw new Error("settings browser distribution does not expose createSettingsSession");
  }

  const session = await module.createSettingsSession(APPEARANCE_DEFINITIONS);
  let persistenceNote = "";
  const stored = localStorage.getItem(SETTINGS_STORAGE_KEY);
  if (stored) {
    try {
      const diagnostics = session.importScope("user", stored);
      if (diagnostics.length > 0) persistenceNote = ` (${diagnostics.length} stored value diagnostics)`;
    } catch (error) {
      persistenceNote = ` (stored values ignored: ${messageFor(error)})`;
    }
  }

  settingsSession = session;
  syncSettingsControls();
  setSettingsControlsDisabled(false);
  applySettingsAppearance();
  settingsStatus.textContent = `Shared settings active${persistenceNote}`;

  colorSchemeControl.addEventListener("change", () =>
    setChoiceSetting("appearance.color_scheme", colorSchemeControl.value),
  );
  contrastControl.addEventListener("change", () =>
    setChoiceSetting("appearance.contrast", contrastControl.value),
  );
  colorVisionControl.addEventListener("change", () =>
    setChoiceSetting("appearance.color_vision", colorVisionControl.value),
  );
  nightModeControl.addEventListener("change", () =>
    setBoolSetting("appearance.night_mode", nightModeControl.checked),
  );
}

function syncSettingsControls() {
  if (!settingsSession) return;
  const values = settingsSession.effectiveValues();
  colorSchemeControl.value = requireSettingValue(values, "appearance.color_scheme", "choice");
  contrastControl.value = requireSettingValue(values, "appearance.contrast", "choice");
  colorVisionControl.value = requireSettingValue(values, "appearance.color_vision", "choice");
  nightModeControl.checked = requireSettingValue(values, "appearance.night_mode", "bool");
}

function setSettingsControlsDisabled(disabled) {
  colorSchemeControl.disabled = disabled;
  contrastControl.disabled = disabled;
  colorVisionControl.disabled = disabled;
  nightModeControl.disabled = disabled;
}

function setChoiceSetting(id, value) {
  try {
    settingsSession?.set(id, { type: "choice", value });
    persistAndApplySettings();
  } catch (error) {
    settingsStatus.textContent = `Settings update rejected: ${messageFor(error)}`;
    syncSettingsControls();
  }
}

function setBoolSetting(id, value) {
  try {
    settingsSession?.set(id, { type: "bool", value });
    persistAndApplySettings();
  } catch (error) {
    settingsStatus.textContent = `Settings update rejected: ${messageFor(error)}`;
    syncSettingsControls();
  }
}

function persistAndApplySettings() {
  if (!settingsSession) return;
  localStorage.setItem(SETTINGS_STORAGE_KEY, settingsSession.exportScope("user"));
  applySettingsAppearance();
  settingsStatus.textContent = "Shared settings active";
}

function applySettingsAppearance() {
  if (!settingsSession) return;
  const values = settingsSession.effectiveValues();
  const colorSchemePreference = requireSettingValue(values, "appearance.color_scheme", "choice");
  const contrastPreference = requireSettingValue(values, "appearance.contrast", "choice");
  const colorVision = requireSettingValue(values, "appearance.color_vision", "choice");
  const nightMode = requireSettingValue(values, "appearance.night_mode", "bool");
  const next = {
    colorScheme:
      colorSchemePreference === "system"
        ? systemColorScheme.matches
          ? "dark"
          : "light"
        : colorSchemePreference,
    contrast: contrastPreference === "system" ? resolveSystemContrast() : contrastPreference,
    colorVision,
    nightMode,
  };
  const changed = JSON.stringify(next) !== JSON.stringify(resolvedAppearance);
  resolvedAppearance = next;
  applyAppearanceDataset();
  if (changed) recreateRenderer();
}

function requireSettingValue(values, id, type) {
  const value = values[id];
  if (!value || value.type !== type) throw new Error(`settings foundation returned an invalid ${id} value`);
  return value.value;
}

function applyAppearanceDataset() {
  const root = document.documentElement;
  root.dataset.theme = resolvedAppearance.colorScheme;
  root.dataset.contrast = resolvedAppearance.contrast;
  root.dataset.colorVision = resolvedAppearance.colorVision;
  root.dataset.nightMode = String(resolvedAppearance.nightMode);
}

function resolveSystemContrast() {
  if (systemHighContrast.matches) return "high";
  if (systemLowContrast.matches) return "low";
  return "normal";
}

function systemAppearanceChanged() {
  if (!settingsSession) {
    const next = {
      ...resolvedAppearance,
      colorScheme: systemColorScheme.matches ? "dark" : "light",
      contrast: resolveSystemContrast(),
    };
    const changed = JSON.stringify(next) !== JSON.stringify(resolvedAppearance);
    resolvedAppearance = next;
    applyAppearanceDataset();
    if (changed) recreateRenderer();
    return;
  }
  applySettingsAppearance();
}

async function initializeInputBindings() {
  const module = await inputBindingsModulePromise;
  if (module.foundationLoadError) throw module.foundationLoadError;
  const { InputRuntimeController, attachKeyboardRuntime, attachMouseRuntime } = module;
  if (
    typeof InputRuntimeController !== "function" ||
    typeof attachKeyboardRuntime !== "function" ||
    typeof attachMouseRuntime !== "function"
  ) {
    throw new Error("input-bindings browser bridge is missing required runtime exports");
  }

  const controller = new InputRuntimeController({
    registry: cityInputRegistry(),
    getActiveContexts: activeInputContexts,
    consumePolicy: "matched",
    onDispatch: handleInputDispatch,
  });
  if (!controller.validationReport.valid) {
    throw new Error(`city input registry is invalid: ${JSON.stringify(controller.validationReport.diagnostics)}`);
  }

  inputController = controller;
  detachKeyboard = attachKeyboardRuntime(controller, {
    mode: "logical",
    ignoreTextEntry: true,
    stopPropagation: false,
  });
  detachMouse = attachMouseRuntime(controller, {
    target: canvas,
    ignoreTextEntry: true,
    stopPropagation: false,
  });
  inputStatus.textContent =
    "Shared input bindings active · Shift + drag/arrows move · wheel/+/− zoom · Home overview · F focus · Esc clear/back";
}

function cityInputRegistry() {
  const context = (id) => ({ op: "context", id });
  const logical = (id, action, value, when, modifiers) => ({
    id,
    action,
    sequence: [
      {
        key: { kind: "logical", value },
        ...(modifiers ? { modifiers } : {}),
      },
    ],
    when,
  });
  const wheel = (id, action, direction, when) => ({
    id,
    action,
    sequence: [{ device: "wheel", direction }],
    when,
  });
  return {
    actions: [
      {
        id: "city.scenario.previous",
        title: "Previous scenario",
        repeatPolicy: "allow",
        allowedDevices: ["keyboard"],
        defaults: [
          logical("city.scenario.previous.arrow-up", "city.scenario.previous", "ArrowUp", context("scenarioPicker")),
          logical("city.scenario.previous.arrow-left", "city.scenario.previous", "ArrowLeft", context("scenarioPicker")),
        ],
      },
      {
        id: "city.scenario.next",
        title: "Next scenario",
        repeatPolicy: "allow",
        allowedDevices: ["keyboard"],
        defaults: [
          logical("city.scenario.next.arrow-down", "city.scenario.next", "ArrowDown", context("scenarioPicker")),
          logical("city.scenario.next.arrow-right", "city.scenario.next", "ArrowRight", context("scenarioPicker")),
        ],
      },
      {
        id: "city.scenario.open",
        title: "Open selected scenario",
        allowedDevices: ["keyboard"],
        defaults: [logical("city.scenario.open.enter", "city.scenario.open", "Enter", context("scenarioPicker"))],
      },
      {
        id: "city.view.panLeft",
        title: "Move view left",
        repeatPolicy: "allow",
        allowedDevices: ["keyboard"],
        defaults: [logical("city.view.pan-left.arrow", "city.view.panLeft", "ArrowLeft", context("cityView"))],
      },
      {
        id: "city.view.panRight",
        title: "Move view right",
        repeatPolicy: "allow",
        allowedDevices: ["keyboard"],
        defaults: [logical("city.view.pan-right.arrow", "city.view.panRight", "ArrowRight", context("cityView"))],
      },
      {
        id: "city.view.panUp",
        title: "Move view up",
        repeatPolicy: "allow",
        allowedDevices: ["keyboard"],
        defaults: [logical("city.view.pan-up.arrow", "city.view.panUp", "ArrowUp", context("cityView"))],
      },
      {
        id: "city.view.panDown",
        title: "Move view down",
        repeatPolicy: "allow",
        allowedDevices: ["keyboard"],
        defaults: [logical("city.view.pan-down.arrow", "city.view.panDown", "ArrowDown", context("cityView"))],
      },
      {
        id: "city.view.zoomIn",
        title: "Zoom in",
        repeatPolicy: "allow",
        allowedDevices: ["keyboard", "mouse"],
        defaults: [
          logical("city.view.zoom-in.equal", "city.view.zoomIn", "=", context("cityView")),
          logical("city.view.zoom-in.plus", "city.view.zoomIn", "+", context("cityView"), { shift: true }),
          wheel("city.view.zoom-in.wheel", "city.view.zoomIn", "up", context("cityView")),
        ],
      },
      {
        id: "city.view.zoomOut",
        title: "Zoom out",
        repeatPolicy: "allow",
        allowedDevices: ["keyboard", "mouse"],
        defaults: [
          logical("city.view.zoom-out.minus", "city.view.zoomOut", "-", context("cityView")),
          wheel("city.view.zoom-out.wheel", "city.view.zoomOut", "down", context("cityView")),
        ],
      },
      {
        id: "city.view.overview",
        title: "Restore overview",
        allowedDevices: ["keyboard"],
        defaults: [
          logical("city.view.overview.home", "city.view.overview", "Home", context("cityView")),
          logical("city.view.overview.zero", "city.view.overview", "0", context("cityView")),
        ],
      },
      {
        id: "city.view.focusSelection",
        title: "Focus selected object",
        allowedDevices: ["keyboard"],
        defaults: [
          logical("city.view.focus-selection.f", "city.view.focusSelection", "f", context("selectionExists")),
        ],
      },
      {
        id: "city.selection.clear",
        title: "Clear selection",
        allowedDevices: ["keyboard"],
        defaults: [logical("city.selection.clear.escape", "city.selection.clear", "Escape", context("selectionExists"))],
      },
      {
        id: "city.scenario.choose",
        title: "Choose another scenario",
        allowedDevices: ["keyboard"],
        defaults: [
          logical("city.scenario.choose.escape", "city.scenario.choose", "Escape", {
            op: "all",
            exprs: [context("cityView"), { op: "not", expr: context("selectionExists") }],
          }),
        ],
      },
    ],
  };
}

function activeInputContexts() {
  const contexts = new Set([appMode === "picker" ? "scenarioPicker" : "cityView"]);
  if (appMode === "city" && selectedEntity) contexts.add("selectionExists");
  return contexts;
}

function handleInputDispatch(dispatch) {
  if (dispatch.phase === "release") return;
  const panStep = KEYBOARD_PAN_STEP / cameraView.zoom;
  switch (dispatch.action) {
    case "city.scenario.previous":
      selectScenarioByOffset(-1).catch(reportInteractionError);
      break;
    case "city.scenario.next":
      selectScenarioByOffset(1).catch(reportInteractionError);
      break;
    case "city.scenario.open":
      setMode("city");
      break;
    case "city.view.panLeft":
      panBy(-panStep, 0);
      break;
    case "city.view.panRight":
      panBy(panStep, 0);
      break;
    case "city.view.panUp":
      panBy(0, panStep);
      break;
    case "city.view.panDown":
      panBy(0, -panStep);
      break;
    case "city.view.zoomIn":
      zoomBy(ZOOM_FACTOR, false);
      break;
    case "city.view.zoomOut":
      zoomBy(1 / ZOOM_FACTOR, false);
      break;
    case "city.view.overview":
      resetView();
      break;
    case "city.view.focusSelection":
      focusSelection();
      break;
    case "city.selection.clear":
      setSelectedEntity(null);
      statusLabel.textContent = "Selection cleared.";
      break;
    case "city.scenario.choose":
      setMode("picker");
      break;
  }
}

function reportInteractionError(error) {
  statusLabel.textContent = messageFor(error);
}

function messageFor(error) {
  return error instanceof Error ? error.message : String(error);
}

function requiredElement(selector, constructor) {
  const element = document.querySelector(selector);
  if (!(element instanceof constructor)) throw new Error(`city-game surface is missing ${selector}`);
  return element;
}

function canvasPointer(event) {
  const rect = canvas.getBoundingClientRect();
  return {
    x: event.clientX - rect.left,
    y: event.clientY - rect.top,
    width: rect.width,
    height: rect.height,
  };
}

function queuePan(deltaX, deltaY) {
  pendingPan.x += deltaX;
  pendingPan.y += deltaY;
  if (panAnimationFrame !== null) return;
  panAnimationFrame = requestAnimationFrame(() => {
    panAnimationFrame = null;
    const delta = pendingPan;
    pendingPan = { x: 0, y: 0 };
    if (delta.x !== 0 || delta.y !== 0) panBy(delta.x, delta.y);
  });
}

function flushQueuedPan() {
  if (panAnimationFrame !== null) {
    cancelAnimationFrame(panAnimationFrame);
    panAnimationFrame = null;
  }
  const delta = pendingPan;
  pendingPan = { x: 0, y: 0 };
  if (delta.x !== 0 || delta.y !== 0) panBy(delta.x, delta.y);
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
    selectScenario(scenario).catch(reportInteractionError);
  });
  buttons.set(scenario.id, button);
  scenarioList.append(button);
}

openScenarioButton.addEventListener("click", () => setMode("city"));
chooseScenarioButton.addEventListener("click", () => setMode("picker"));
viewOverviewButton.addEventListener("click", () => resetView());
viewZoomInButton.addEventListener("click", () => zoomBy(ZOOM_FACTOR));
viewZoomOutButton.addEventListener("click", () => zoomBy(1 / ZOOM_FACTOR));
viewFocusButton.addEventListener("click", focusSelection);

canvas.addEventListener("pointerdown", (event) => {
  if (appMode !== "city" || event.button !== 0) return;
  const pointer = canvasPointer(event);
  lastCanvasPointer = pointer;
  pointerGesture = {
    pointerId: event.pointerId,
    startX: pointer.x,
    startY: pointer.y,
    lastX: pointer.x,
    lastY: pointer.y,
    dragging: false,
  };
  canvas.setPointerCapture(event.pointerId);
});

canvas.addEventListener("pointermove", (event) => {
  const pointer = canvasPointer(event);
  lastCanvasPointer = pointer;
  if (!pointerGesture || pointerGesture.pointerId !== event.pointerId || appMode !== "city") return;
  if (!event.shiftKey) {
    pointerGesture.lastX = pointer.x;
    pointerGesture.lastY = pointer.y;
    return;
  }

  const totalDistance = Math.hypot(pointer.x - pointerGesture.startX, pointer.y - pointerGesture.startY);
  if (!pointerGesture.dragging && totalDistance >= DRAG_THRESHOLD_PX) {
    pointerGesture.dragging = true;
    canvas.dataset.dragging = "true";
  }
  if (!pointerGesture.dragging) return;

  const deltaX = pointer.x - pointerGesture.lastX;
  const deltaY = pointer.y - pointerGesture.lastY;
  pointerGesture.lastX = pointer.x;
  pointerGesture.lastY = pointer.y;
  queuePan(
    -deltaX / Math.max(1, pointer.width) / cameraView.zoom,
    deltaY / Math.max(1, pointer.height) / cameraView.zoom,
  );
});

canvas.addEventListener("pointerup", (event) => {
  if (!pointerGesture || pointerGesture.pointerId !== event.pointerId) return;
  const pointer = canvasPointer(event);
  lastCanvasPointer = pointer;
  const totalDistance = Math.hypot(pointer.x - pointerGesture.startX, pointer.y - pointerGesture.startY);
  const wasDragging = pointerGesture.dragging;
  pointerGesture = null;
  delete canvas.dataset.dragging;
  if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
  flushQueuedPan();
  if (!wasDragging && totalDistance < DRAG_THRESHOLD_PX) pickAtPointer(pointer);
});

canvas.addEventListener("pointercancel", (event) => {
  if (!pointerGesture || pointerGesture.pointerId !== event.pointerId) return;
  pointerGesture = null;
  delete canvas.dataset.dragging;
  pendingPan = { x: 0, y: 0 };
  if (panAnimationFrame !== null) {
    cancelAnimationFrame(panAnimationFrame);
    panAnimationFrame = null;
  }
});

systemColorScheme.addEventListener("change", systemAppearanceChanged);
systemHighContrast.addEventListener("change", systemAppearanceChanged);
systemLowContrast.addEventListener("change", systemAppearanceChanged);

const requestedUrl = new URL(window.location.href);
const requestedId = requestedUrl.searchParams.get("scenario");
const requestedMode = requestedUrl.searchParams.get("mode");
const initialScenario = scenarios.get(requestedId) ?? scenarioOrder[0];
await selectScenario(initialScenario);
if (requestedMode === "city") setMode("city");

void initializeSettings().catch((error) => {
  settingsStatus.textContent = `Shared settings unavailable: ${messageFor(error)}`;
  setSettingsControlsDisabled(true);
});
void initializeInputBindings().catch((error) => {
  inputStatus.textContent = `Shared input bindings unavailable: ${messageFor(error)}`;
});

const observer = new ResizeObserver(render);
observer.observe(canvas);
window.addEventListener("pagehide", () => {
  currentSession = null;
  currentCanonicalScenario = null;
  settingsSession?.dispose();
  inputController?.reset("pagehide");
  detachKeyboard?.();
  detachMouse?.();
  if (panAnimationFrame !== null) cancelAnimationFrame(panAnimationFrame);
  observer.disconnect();
  renderer.dispose();
});
