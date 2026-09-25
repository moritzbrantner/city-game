import { CityRenderCache } from "./render-cache.js";

const encoder = new TextEncoder();
const decoder = new TextDecoder();

const OVERVIEW_VIEW = Object.freeze({ panX: 0, panY: 0, zoom: 1 });

export async function createCityGameRuntime(url = "./city-game-core.wasm") {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`failed to load city-game WASM: ${response.status}`);
  }
  const { instance } = await WebAssembly.instantiate(await response.arrayBuffer(), {});
  return new CityGameRuntime(instance.exports);
}

export class CityGameRuntime {
  #exports;

  constructor(exports) {
    const required = [
      "memory",
      "city_game_alloc",
      "city_game_free",
      "city_game_session_create",
      "city_game_session_destroy",
      "city_game_session_execute",
      "city_game_session_query",
      "city_game_session_prepare_render",
      "city_game_render_camera",
    ];
    for (const name of required) {
      if (!(name in exports)) {
        throw new Error(`city-game WASM is missing export ${name}`);
      }
    }
    this.#exports = exports;
  }

  createSession(scenario) {
    const created = requireSuccess(this.#createSession(scenario));
    const handle = requireSessionHandle(created.handle);
    const renderCache = new CityRenderCache(
      (aspect) => requireSuccess(this.#prepareSession(handle, aspect)),
      (overview, view) => requireSuccess(this.#renderCamera(overview, view)).camera,
    );

    return new CityGameSession(
      (command) => {
        const commandResponse = requireSuccess(this.#executeSession(handle, command));
        const outcome = commandResponse.outcome;
        // Only the authoritative explicit no-op receipt permits retaining geometry.
        if (outcome?.kind !== "planning" || outcome.outcome !== "unchanged") {
          renderCache.invalidate();
        }
        return outcome;
      },
      (query) => requireSuccess(this.#querySession(handle, query)).result,
      (aspect, view) => renderCache.renderFrame(aspect, view),
      () => {
        renderCache.invalidate();
        this.#exports.city_game_session_destroy(handle);
      },
    );
  }

  #createSession(scenario) {
    return this.#call([scenario], (input) =>
      this.#exports.city_game_session_create(input.ptr, input.len),
    );
  }

  #executeSession(handle, command) {
    return this.#call([command], (input) =>
      this.#exports.city_game_session_execute(handle, input.ptr, input.len),
    );
  }

  #querySession(handle, query) {
    return this.#call([query], (input) =>
      this.#exports.city_game_session_query(handle, input.ptr, input.len),
    );
  }

  #prepareSession(handle, aspect) {
    return this.#read(this.#exports.city_game_session_prepare_render(handle, aspect));
  }

  #renderCamera(overview, view) {
    return this.#call([overview, view], (overviewInput, viewInput) =>
      this.#exports.city_game_render_camera(
        overviewInput.ptr,
        overviewInput.len,
        viewInput.ptr,
        viewInput.len,
      ),
    );
  }

  #call(values, invoke) {
    const inputs = [];
    try {
      // Keep earlier inputs covered by cleanup if a later serialization/allocation fails.
      for (const value of values) inputs.push(this.#write(value));
      return this.#read(invoke(...inputs));
    } finally {
      for (const input of inputs) {
        this.#exports.city_game_free(input.ptr, input.len);
      }
    }
  }

  #write(value) {
    const bytes = encoder.encode(JSON.stringify(value));
    const ptr = this.#exports.city_game_alloc(bytes.length);
    if (bytes.length > 0 && ptr === 0) {
      throw new Error("city-game WASM failed to allocate input memory");
    }
    new Uint8Array(this.#exports.memory.buffer, ptr, bytes.length).set(bytes);
    return { ptr, len: bytes.length };
  }

  #read(packedResult) {
    const packed = BigInt.asUintN(64, packedResult);
    const ptr = Number(packed & 0xffffffffn);
    const len = Number(packed >> 32n);
    if (len === 0) {
      throw new Error("city-game WASM returned an empty response");
    }

    try {
      const json = decoder.decode(new Uint8Array(this.#exports.memory.buffer, ptr, len));
      return JSON.parse(json);
    } finally {
      this.#exports.city_game_free(ptr, len);
    }
  }
}

class CityGameSession {
  #executeCommand;
  #queryState;
  #render;
  #disposeSession;
  #disposed = false;

  constructor(executeCommand, queryState, render, disposeSession) {
    this.#executeCommand = executeCommand;
    this.#queryState = queryState;
    this.#render = render;
    this.#disposeSession = disposeSession;
  }

  execute(command) {
    this.#requireActive();
    return this.#executeCommand(command);
  }

  query(query) {
    this.#requireActive();
    return this.#queryState(query);
  }

  renderFrame(aspect, view = OVERVIEW_VIEW) {
    this.#requireActive();
    return this.#render(aspect, view);
  }

  dispose() {
    if (this.#disposed) return;
    this.#disposed = true;
    this.#disposeSession();
  }

  #requireActive() {
    if (this.#disposed) {
      throw new Error("city-game session has been disposed");
    }
  }
}

function requireSessionHandle(value) {
  const handle = Number(value);
  if (!Number.isInteger(handle) || handle <= 0 || handle > 0xffffffff) {
    throw new Error("city-game WASM returned an invalid session handle");
  }
  return handle;
}

function requireSuccess(response) {
  if (response?.ok !== true) {
    throw new Error(response?.error ?? "city-game WASM operation failed");
  }
  return response;
}
