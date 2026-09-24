const ROAD_SEGMENT_SUFFIX = /^(.*)\/road-segment-\d+$/;

/**
 * Resolve a renderer node id to the authoritative city entity id.
 */
export function entityIdForNode(nodeId) {
  const roadSegment = ROAD_SEGMENT_SUFFIX.exec(nodeId);
  return roadSegment ? roadSegment[1] : nodeId;
}

/**
 * Validate the smallest surface whose identity actually changed.
 * The runtime owns retained node-array identity; a different node array must cross
 * the full renderer-frame validation boundary.
 */
export function validateSessionFrame(previousFrame, nextFrame, validateFrame, validateCamera) {
  if (previousFrame && nextFrame.nodes === previousFrame.nodes) {
    validateCamera(nextFrame.camera);
    return nextFrame;
  }
  return validateFrame(nextFrame);
}

/**
 * Presentation cache between city-game state and the reusable 3d-lab renderer.
 * It never owns authoritative city state. Scene mutations use full render();
 * camera-only frames use renderCamera() against the already-submitted scene.
 */
export class CityFramePresenter {
  #renderer;
  #submittedNodes = null;
  #selectionSourceNodes = null;
  #selectionId = null;
  #selectionAccent = null;
  #selectionNodes = null;

  constructor(renderer) {
    this.#renderer = renderer;
  }

  setRenderer(renderer) {
    this.#renderer = renderer;
    // A new GPU renderer has no submitted scene even when presentation data is reusable.
    this.#submittedNodes = null;
  }

  render(frame, selectionId = null, accent = null) {
    const presented = this.#present(frame, selectionId, accent);
    if (presented.nodes === this.#submittedNodes) {
      return {
        path: "camera",
        observations: this.#renderer.renderCamera(presented.camera),
        frame: presented,
      };
    }

    const observations = this.#renderer.render(presented);
    // Publish only after the full renderer accepted/reconciled the scene.
    this.#submittedNodes = presented.nodes;
    return { path: "scene", observations, frame: presented };
  }

  #present(frame, selectionId, accent) {
    if (selectionId === null) return frame;
    if (
      this.#selectionSourceNodes === frame.nodes &&
      this.#selectionId === selectionId &&
      this.#selectionAccent === accent &&
      this.#selectionNodes !== null
    ) {
      return { ...frame, nodes: this.#selectionNodes };
    }

    const nodes = [];
    let changed = false;
    for (const node of frame.nodes) {
      if (entityIdForNode(node.id) !== selectionId) {
        nodes.push(node);
        continue;
      }

      changed = true;
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

    this.#selectionSourceNodes = frame.nodes;
    this.#selectionId = selectionId;
    this.#selectionAccent = accent;
    this.#selectionNodes = changed ? nodes : frame.nodes;
    return changed ? { ...frame, nodes } : frame;
  }
}
