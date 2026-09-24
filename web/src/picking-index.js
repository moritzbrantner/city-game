import { createWorldProjector } from "@moritzbrantner/three-d-renderer";
import { entityIdForNode } from "./frame-presenter.js";

const TARGET_ITEMS_PER_CELL = 8;
const MIN_GRID_SIZE = 8;
const MAX_GRID_SIZE = 96;
const MAX_CELL_REFERENCES_PER_ITEM = 64;
const QUERY_EPSILON = 1e-6;

export class CityPickingIndex {
  #nodes;
  #viewMatrix;
  #projectionInvariant;
  #overviewProjection;
  #aspect;
  #items = [];
  #entityItems = new Map();
  #grid = [];
  #gridSize = 1;
  #sceneBounds = null;
  #wideItems = [];
  #marks = new Uint32Array(0);
  #generation = 0;
  #buildObservations;

  constructor(frame, view) {
    requireFrame(frame);
    const buildView = normalizeView(view);
    this.#nodes = frame.nodes;
    this.#viewMatrix = [...frame.camera.viewMatrix];
    this.#aspect = frame.camera.aspect;
    this.#projectionInvariant = frame.camera.projectionMatrix.map((value, index) =>
      isViewProjectionComponent(index) ? null : value,
    );
    this.#overviewProjection = overviewProjection(frame.camera.projectionMatrix, buildView);

    const buildWork = {
      nodeVisits: 0,
      projectionCalls: 0,
      indexedNodes: 0,
      skippedNodes: 0,
      cellReferences: 0,
      wideItems: 0,
    };
    const buildProjector = createWorldProjector(frame.camera, { width: 1, height: 1 });

    for (const node of frame.nodes) {
      buildWork.nodeVisits++;
      const bounds = projectNodeBoundsWithProjector(node, buildProjector, buildWork);
      if (!bounds) {
        buildWork.skippedNodes++;
        continue;
      }

      const canonical = boundsToOverview(bounds, buildView);
      const item = {
        node,
        entityId: entityIdForNode(node.id),
        minX: canonical.minX,
        maxX: canonical.maxX,
        minY: canonical.minY,
        maxY: canonical.maxY,
        depth: bounds.depth,
        area: Math.max(
          Number.EPSILON,
          (canonical.maxX - canonical.minX) * (canonical.maxY - canonical.minY),
        ),
      };
      const itemIndex = this.#items.length;
      this.#items.push(item);
      buildWork.indexedNodes++;
      extendBounds(this.#sceneBounds ??= {
        minX: item.minX,
        maxX: item.maxX,
        minY: item.minY,
        maxY: item.maxY,
      }, item);

      const entityItems = this.#entityItems.get(item.entityId) ?? [];
      entityItems.push(itemIndex);
      this.#entityItems.set(item.entityId, entityItems);
    }

    this.#gridSize = this.#items.length === 0
      ? 1
      : clamp(
        Math.ceil(Math.sqrt(this.#items.length / TARGET_ITEMS_PER_CELL)),
        MIN_GRID_SIZE,
        MAX_GRID_SIZE,
      );
    this.#grid = Array.from({ length: this.#gridSize * this.#gridSize }, () => []);
    this.#marks = new Uint32Array(this.#items.length);

    if (this.#sceneBounds) {
      for (let itemIndex = 0; itemIndex < this.#items.length; itemIndex++) {
        const range = this.#cellRange(this.#items[itemIndex]);
        const cellCount = (range.maxX - range.minX + 1) * (range.maxY - range.minY + 1);
        if (cellCount > MAX_CELL_REFERENCES_PER_ITEM) {
          this.#wideItems.push(itemIndex);
          continue;
        }
        for (let y = range.minY; y <= range.maxY; y++) {
          for (let x = range.minX; x <= range.maxX; x++) {
            this.#grid[y * this.#gridSize + x].push(itemIndex);
            buildWork.cellReferences++;
          }
        }
      }
    }
    const byAreaThenId = (leftIndex, rightIndex) =>
      this.#items[leftIndex].area - this.#items[rightIndex].area ||
      this.#items[leftIndex].node.id.localeCompare(this.#items[rightIndex].node.id);
    for (const cell of this.#grid) cell.sort(byAreaThenId);
    this.#wideItems.sort(byAreaThenId);

    buildWork.wideItems = this.#wideItems.length;
    this.#buildObservations = Object.freeze({
      ...buildWork,
      gridSize: this.#gridSize,
      gridCells: this.#grid.length,
    });
  }

  get buildObservations() {
    return this.#buildObservations;
  }

  matches(frame, view) {
    return (
      frame?.nodes === this.#nodes &&
      frame?.camera?.aspect === this.#aspect &&
      matrixEquals(frame.camera.viewMatrix, this.#viewMatrix) &&
      projectionMatchesView(
        frame.camera.projectionMatrix,
        this.#projectionInvariant,
        this.#overviewProjection,
        normalizeView(view),
      )
    );
  }

  pick(frame, view, pointer, kindForEntity = () => undefined) {
    requireFrame(frame);
    const normalizedView = normalizeView(view);
    const normalizedPointer = normalizePointer(pointer);
    if (!this.matches(frame, normalizedView)) {
      const fallback = pickNodeLinear(frame, normalizedPointer, kindForEntity);
      return {
        node: fallback.node,
        observations: {
          path: "linear-fallback",
          gridCellVisits: 0,
          candidateIndexVisits: 0,
          candidateBoundsEvaluations: fallback.observations.boundsEvaluations,
          projectionCalls: fallback.observations.projectionCalls,
          fullSceneNodeVisits: fallback.observations.nodeVisits,
        },
      };
    }

    const slop = pickSlop(normalizedPointer.pointerType);
    const center = pointerToOverview(normalizedPointer, normalizedView);
    const query = {
      minX: center.x - slop / normalizedPointer.width / normalizedView.zoom - QUERY_EPSILON,
      maxX: center.x + slop / normalizedPointer.width / normalizedView.zoom + QUERY_EPSILON,
      minY: center.y - slop / normalizedPointer.height / normalizedView.zoom - QUERY_EPSILON,
      maxY: center.y + slop / normalizedPointer.height / normalizedView.zoom + QUERY_EPSILON,
    };
    const work = {
      path: "indexed",
      gridCellVisits: 0,
      candidateIndexVisits: 0,
      candidateBoundsEvaluations: 0,
      projectionCalls: 0,
      fullSceneNodeVisits: 0,
      directHit: false,
    };
    const direct = this.#pickContainingPoint(
      center,
      normalizedView,
      normalizedPointer,
      kindForEntity,
      work,
    );
    if (direct) {
      work.directHit = true;
      return { node: direct.node, observations: work };
    }

    const candidates = this.#query(query, work);
    let best = null;
    for (const itemIndex of candidates) {
      const item = this.#items[itemIndex];
      const bounds = screenBoundsFromOverview(
        item,
        normalizedView,
        normalizedPointer.width,
        normalizedPointer.height,
      );
      work.candidateBoundsEvaluations++;
      if (!pointerIntersectsBounds(normalizedPointer, bounds, slop)) continue;
      const candidate = scoredCandidate(
        item.node,
        item.entityId,
        kindForEntity,
        normalizedPointer,
        bounds,
      );
      if (!best || compareCandidates(candidate, best) < 0) best = candidate;
    }

    return { node: best?.node ?? null, observations: work };
  }

  screenBoundsForEntity(frame, view, entityId, width, height) {
    requireFrame(frame);
    const normalizedView = normalizeView(view);
    requireViewport(width, height);
    const work = {
      path: "indexed",
      nodeVisits: 0,
      boundsEvaluations: 0,
      projectionCalls: 0,
    };
    if (!this.matches(frame, normalizedView)) {
      const fallback = entityScreenBoundsLinear(frame, entityId, width, height);
      return {
        bounds: fallback.bounds,
        observations: { ...fallback.observations, path: "linear-fallback" },
      };
    }

    const indices = this.#entityItems.get(entityId) ?? [];
    const projector = createWorldProjector(frame.camera, { width, height });
    let combined = null;
    for (const itemIndex of indices) {
      work.nodeVisits++;
      const bounds = projectNodeBoundsWithProjector(
        this.#items[itemIndex].node,
        projector,
        work,
      );
      work.boundsEvaluations++;
      if (!bounds) continue;
      combined = combined
        ? unionScreenBounds(combined, bounds)
        : { minX: bounds.minX, maxX: bounds.maxX, minY: bounds.minY, maxY: bounds.maxY };
    }
    return { bounds: combined, observations: work };
  }

  #pickContainingPoint(center, view, pointer, kindForEntity, work) {
    if (!this.#sceneBounds || !pointInsideBounds(center, this.#sceneBounds)) return null;
    const range = this.#cellRange({
      minX: center.x,
      maxX: center.x,
      minY: center.y,
      maxY: center.y,
    });
    const cellItems = this.#grid[range.minY * this.#gridSize + range.minX];
    let cellOffset = 0;
    let wideOffset = 0;
    let best = null;
    let bestArea = Infinity;

    while (cellOffset < cellItems.length || wideOffset < this.#wideItems.length) {
      const cellIndex = cellItems[cellOffset];
      const wideIndex = this.#wideItems[wideOffset];
      let itemIndex;
      if (wideIndex === undefined) {
        itemIndex = cellIndex;
        cellOffset++;
      } else if (cellIndex === undefined) {
        itemIndex = wideIndex;
        wideOffset++;
      } else {
        const cellItem = this.#items[cellIndex];
        const wideItem = this.#items[wideIndex];
        if (
          cellItem.area < wideItem.area ||
          (
            cellItem.area === wideItem.area &&
            cellItem.node.id.localeCompare(wideItem.node.id) <= 0
          )
        ) {
          itemIndex = cellIndex;
          cellOffset++;
        } else {
          itemIndex = wideIndex;
          wideOffset++;
        }
      }

      const item = this.#items[itemIndex];
      if (item.area > bestArea) break;
      work.candidateIndexVisits++;
      if (!pointInsideBounds(center, item)) continue;

      const bounds = screenBoundsFromOverview(item, view, pointer.width, pointer.height);
      work.candidateBoundsEvaluations++;
      const candidate = scoredCandidate(
        item.node,
        item.entityId,
        kindForEntity,
        pointer,
        bounds,
      );
      if (!best || compareCandidates(candidate, best) < 0) {
        best = candidate;
        bestArea = item.area;
      }
    }
    return best;
  }

  #query(query, work) {
    if (this.#items.length === 0) return [];
    this.#generation = (this.#generation + 1) >>> 0;
    if (this.#generation === 0) {
      this.#marks.fill(0);
      this.#generation = 1;
    }

    const result = [];
    const add = (itemIndex) => {
      work.candidateIndexVisits++;
      if (this.#marks[itemIndex] === this.#generation) return;
      this.#marks[itemIndex] = this.#generation;
      const item = this.#items[itemIndex];
      if (!rectanglesOverlap(item, query)) return;
      result.push(itemIndex);
    };

    for (const itemIndex of this.#wideItems) add(itemIndex);
    if (!this.#sceneBounds || !rectanglesOverlap(this.#sceneBounds, query)) return result;

    const range = this.#cellRange(query);
    for (let y = range.minY; y <= range.maxY; y++) {
      for (let x = range.minX; x <= range.maxX; x++) {
        work.gridCellVisits++;
        for (const itemIndex of this.#grid[y * this.#gridSize + x]) add(itemIndex);
      }
    }
    return result;
  }

  #cellRange(bounds) {
    const scene = this.#sceneBounds;
    if (!scene) return { minX: 0, maxX: 0, minY: 0, maxY: 0 };
    const width = Math.max(scene.maxX - scene.minX, Number.EPSILON);
    const height = Math.max(scene.maxY - scene.minY, Number.EPSILON);
    const cell = (value, minimum, span) =>
      clamp(Math.floor(((value - minimum) / span) * this.#gridSize), 0, this.#gridSize - 1);
    return {
      minX: cell(bounds.minX, scene.minX, width),
      maxX: cell(bounds.maxX, scene.minX, width),
      minY: cell(bounds.minY, scene.minY, height),
      maxY: cell(bounds.maxY, scene.minY, height),
    };
  }
}

export function pickNodeLinear(frame, pointer, kindForEntity = () => undefined) {
  requireFrame(frame);
  const normalizedPointer = normalizePointer(pointer);
  const slop = pickSlop(normalizedPointer.pointerType);
  const work = { nodeVisits: 0, boundsEvaluations: 0, projectionCalls: 0 };
  const projector = createWorldProjector(frame.camera, {
    width: normalizedPointer.width,
    height: normalizedPointer.height,
  });
  let best = null;
  for (const node of frame.nodes) {
    work.nodeVisits++;
    const bounds = projectNodeBoundsWithProjector(node, projector, work);
    work.boundsEvaluations++;
    if (!bounds || !pointerIntersectsBounds(normalizedPointer, bounds, slop)) continue;
    const entityId = entityIdForNode(node.id);
    const candidate = scoredCandidate(
      node,
      entityId,
      kindForEntity,
      normalizedPointer,
      bounds,
    );
    if (!best || compareCandidates(candidate, best) < 0) best = candidate;
  }
  return { node: best?.node ?? null, observations: work };
}

export function entityScreenBoundsLinear(frame, entityId, width, height) {
  requireFrame(frame);
  requireViewport(width, height);
  const work = { nodeVisits: 0, boundsEvaluations: 0, projectionCalls: 0 };
  const projector = createWorldProjector(frame.camera, { width, height });
  let combined = null;
  for (const node of frame.nodes) {
    work.nodeVisits++;
    if (entityIdForNode(node.id) !== entityId) continue;
    const bounds = projectNodeBoundsWithProjector(node, projector, work);
    work.boundsEvaluations++;
    if (!bounds) continue;
    combined = combined
      ? unionScreenBounds(combined, bounds)
      : { minX: bounds.minX, maxX: bounds.maxX, minY: bounds.minY, maxY: bounds.maxY };
  }
  return { bounds: combined, observations: work };
}

export function projectNodeBounds(camera, node, width, height, work = null) {
  requireViewport(width, height);
  return projectNodeBoundsWithProjector(
    node,
    createWorldProjector(camera, { width, height }),
    work,
  );
}

function projectNodeBoundsWithProjector(node, project, work = null) {
  if (node.geometry?.kind !== "box") return null;
  const [sizeX, sizeY, sizeZ] = node.geometry.size;
  const projected = [];
  for (const x of [-sizeX * 0.5, sizeX * 0.5]) {
    for (const y of [-sizeY * 0.5, sizeY * 0.5]) {
      for (const z of [-sizeZ * 0.5, sizeZ * 0.5]) {
        const worldPoint = transformNodePoint(node, [x, y, z]);
        const point = project(worldPoint);
        if (work) work.projectionCalls++;
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

function scoredCandidate(node, entityId, kindForEntity, pointer, bounds) {
  const centerX = (bounds.minX + bounds.maxX) * 0.5;
  const centerY = (bounds.minY + bounds.maxY) * 0.5;
  const dx = Math.max(bounds.minX - pointer.x, 0, pointer.x - bounds.maxX);
  const dy = Math.max(bounds.minY - pointer.y, 0, pointer.y - bounds.maxY);
  return {
    node,
    hitDistanceSquared: dx * dx + dy * dy,
    centerDistanceSquared: (pointer.x - centerX) ** 2 + (pointer.y - centerY) ** 2,
    area: Math.max(1, (bounds.maxX - bounds.minX) * (bounds.maxY - bounds.minY)),
    kindPriority: pickKindPriority(kindForEntity(entityId)),
    depth: bounds.depth,
  };
}

function compareCandidates(left, right) {
  return (
    left.hitDistanceSquared - right.hitDistanceSquared ||
    left.area - right.area ||
    left.kindPriority - right.kindPriority ||
    left.centerDistanceSquared - right.centerDistanceSquared ||
    left.depth - right.depth ||
    left.node.id.localeCompare(right.node.id)
  );
}

function pickKindPriority(kind) {
  switch (kind) {
    case "Building":
      return 0;
    case "Road":
      return 1;
    case "Water":
      return 2;
    case "Land use":
      return 3;
    default:
      return 4;
  }
}

function pickSlop(pointerType) {
  return pointerType === "touch" ? 24 : 16;
}

function pointerIntersectsBounds(pointer, bounds, slop) {
  return !(
    pointer.x < bounds.minX - slop ||
    pointer.x > bounds.maxX + slop ||
    pointer.y < bounds.minY - slop ||
    pointer.y > bounds.maxY + slop
  );
}

function pointerToOverview(pointer, view) {
  return {
    x: 0.5 + view.panX + (pointer.x / pointer.width - 0.5) / view.zoom,
    y: 0.5 - view.panY + (pointer.y / pointer.height - 0.5) / view.zoom,
  };
}

function screenBoundsFromOverview(item, view, width, height) {
  const minX = (0.5 + view.zoom * (item.minX - 0.5 - view.panX)) * width;
  const maxX = (0.5 + view.zoom * (item.maxX - 0.5 - view.panX)) * width;
  const minY = (0.5 + view.zoom * (item.minY - 0.5 + view.panY)) * height;
  const maxY = (0.5 + view.zoom * (item.maxY - 0.5 + view.panY)) * height;
  return { minX, maxX, minY, maxY, depth: item.depth };
}

function boundsToOverview(bounds, view) {
  return {
    minX: 0.5 + view.panX + (bounds.minX - 0.5) / view.zoom,
    maxX: 0.5 + view.panX + (bounds.maxX - 0.5) / view.zoom,
    minY: 0.5 - view.panY + (bounds.minY - 0.5) / view.zoom,
    maxY: 0.5 - view.panY + (bounds.maxY - 0.5) / view.zoom,
  };
}

function overviewProjection(projection, view) {
  return {
    scaleX: projection[0] / view.zoom,
    scaleY: projection[5] / view.zoom,
    offsetX: projection[12] / view.zoom + 2 * view.panX,
    offsetY: projection[13] / view.zoom + 2 * view.panY,
  };
}

function projectionMatchesView(projection, invariant, overview, view) {
  if (!Array.isArray(projection) || projection.length !== 16) return false;
  const expected = {
    0: overview.scaleX * view.zoom,
    5: overview.scaleY * view.zoom,
    12: (overview.offsetX - 2 * view.panX) * view.zoom,
    13: (overview.offsetY - 2 * view.panY) * view.zoom,
  };
  for (let index = 0; index < 16; index++) {
    if (isViewProjectionComponent(index)) {
      if (!nearlyEqual(projection[index], expected[index])) return false;
      continue;
    }
    if (!nearlyEqual(projection[index], invariant[index])) return false;
  }
  return true;
}

function isViewProjectionComponent(index) {
  return index === 0 || index === 5 || index === 12 || index === 13;
}

function nearlyEqual(left, right) {
  if (!Number.isFinite(left) || !Number.isFinite(right)) return false;
  const scale = Math.max(1, Math.abs(left), Math.abs(right));
  return Math.abs(left - right) <= scale * 2e-5;
}

function matrixEquals(left, right) {
  return (
    Array.isArray(left) &&
    left.length === right.length &&
    left.every((value, index) => value === right[index])
  );
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
  if (!Number.isFinite(worldW) || Math.abs(worldW) <= Number.EPSILON) {
    return [worldX, worldY, worldZ];
  }
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

function normalizeView(view) {
  const normalized = {
    panX: Number(view?.panX),
    panY: Number(view?.panY),
    zoom: Number(view?.zoom),
  };
  if (
    !Number.isFinite(normalized.panX) ||
    !Number.isFinite(normalized.panY) ||
    !Number.isFinite(normalized.zoom) ||
    normalized.zoom <= 0
  ) {
    throw new Error("picking view must contain finite pan and positive zoom");
  }
  return normalized;
}

function normalizePointer(pointer) {
  const normalized = {
    x: Number(pointer?.x),
    y: Number(pointer?.y),
    width: Number(pointer?.width),
    height: Number(pointer?.height),
    pointerType: pointer?.pointerType ?? "mouse",
  };
  if (
    ![normalized.x, normalized.y, normalized.width, normalized.height].every(Number.isFinite) ||
    normalized.width <= 0 ||
    normalized.height <= 0
  ) {
    throw new Error("picking pointer must contain finite coordinates and positive viewport size");
  }
  return normalized;
}

function requireFrame(frame) {
  if (!frame || !Array.isArray(frame.nodes) || !frame.camera) {
    throw new Error("picking requires a render frame");
  }
}

function requireViewport(width, height) {
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    throw new Error("picking viewport must be finite and positive");
  }
}

function pointInsideBounds(point, bounds) {
  return (
    point.x >= bounds.minX &&
    point.x <= bounds.maxX &&
    point.y >= bounds.minY &&
    point.y <= bounds.maxY
  );
}

function rectanglesOverlap(left, right) {
  return !(
    left.maxX < right.minX ||
    left.minX > right.maxX ||
    left.maxY < right.minY ||
    left.minY > right.maxY
  );
}

function extendBounds(target, value) {
  target.minX = Math.min(target.minX, value.minX);
  target.maxX = Math.max(target.maxX, value.maxX);
  target.minY = Math.min(target.minY, value.minY);
  target.maxY = Math.max(target.maxY, value.maxY);
}

function unionScreenBounds(left, right) {
  return {
    minX: Math.min(left.minX, right.minX),
    maxX: Math.max(left.maxX, right.maxX),
    minY: Math.min(left.minY, right.minY),
    maxY: Math.max(left.maxY, right.maxY),
  };
}

function clamp(value, minimum, maximum) {
  return Math.min(maximum, Math.max(minimum, value));
}
