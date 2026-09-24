const OVERVIEW_VIEW = Object.freeze({ panX: 0, panY: 0, zoom: 1 });

/** One disposable scene per session. Camera math remains in the Rust adapter. */
export class CityRenderCache {
  #prepare;
  #camera;
  #prepared = null;
  #aspect = null;
  #view = null;
  #frame = null;

  constructor(prepare, camera) {
    this.#prepare = prepare;
    this.#camera = camera;
  }

  invalidate() {
    this.#prepared = null;
    this.#aspect = null;
    this.#view = null;
    this.#frame = null;
  }

  renderFrame(aspect, view = OVERVIEW_VIEW) {
    if (!Number.isFinite(aspect) || aspect <= 0) {
      throw new Error("render aspect must be finite and positive");
    }
    view ??= OVERVIEW_VIEW;
    const sameAspect = this.#prepared !== null && aspect === this.#aspect;
    if (sameAspect && sameView(view, this.#view)) return this.#frame;

    const prepared = sameAspect ? this.#prepared : this.#prepare(aspect);
    const camera = sameView(view, OVERVIEW_VIEW)
      ? prepared.frame.camera
      : this.#camera(prepared.overview, view);
    const frame = { camera, nodes: prepared.frame.nodes };

    // Publish only after both operations succeed; invalid input cannot poison a cache.
    this.#prepared = prepared;
    this.#aspect = aspect;
    this.#view = { panX: view.panX, panY: view.panY, zoom: view.zoom };
    this.#frame = frame;
    return frame;
  }
}

function sameView(left, right) {
  return right !== null && left.panX === right.panX &&
    left.panY === right.panY && left.zoom === right.zoom;
}
