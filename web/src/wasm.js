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

class CityGameRuntime {
  #exports;

  constructor(exports) {
    const required = [
      "memory",
      "city_game_alloc",
      "city_game_free",
      "city_game_new_save",
      "city_game_execute",
      "city_game_query",
      "city_game_render_frame",
      "city_game_render_frame_view",
    ];
    for (const name of required) {
      if (!(name in exports)) {
        throw new Error(`city-game WASM is missing export ${name}`);
      }
    }
    this.#exports = exports;
  }

  createSession(scenario) {
    const response = requireSuccess(this.#newSave(scenario));
    let save = response.save;

    return new CityGameSession(
      (command) => {
        const commandResponse = requireSuccess(this.#execute(save, command));
        save = commandResponse.save;
        return commandResponse.outcome;
      },
      (query) => requireSuccess(this.#query(save, query)).result,
      (aspect, view) => requireSuccess(this.#renderFrame(save, aspect, view)).frame,
    );
  }

  #newSave(scenario) {
    return this.#call([scenario], (input) =>
      this.#exports.city_game_new_save(input.ptr, input.len),
    );
  }

  #execute(save, command) {
    return this.#call([save, command], (saveInput, commandInput) =>
      this.#exports.city_game_execute(
        saveInput.ptr,
        saveInput.len,
        commandInput.ptr,
        commandInput.len,
      ),
    );
  }

  #query(save, query) {
    return this.#call([save, query], (saveInput, queryInput) =>
      this.#exports.city_game_query(
        saveInput.ptr,
        saveInput.len,
        queryInput.ptr,
        queryInput.len,
      ),
    );
  }

  #renderFrame(save, aspect, view = OVERVIEW_VIEW) {
    if (!Number.isFinite(aspect) || aspect <= 0) {
      throw new Error("render aspect must be finite and positive");
    }
    if (view === undefined || view === null) {
      return this.#call([save], (input) =>
        this.#exports.city_game_render_frame(input.ptr, input.len, aspect),
      );
    }
    return this.#call([save, view], (saveInput, viewInput) =>
      this.#exports.city_game_render_frame_view(
        saveInput.ptr,
        saveInput.len,
        viewInput.ptr,
        viewInput.len,
        aspect,
      ),
    );
  }

  #call(values, invoke) {
    const inputs = values.map((value) => this.#write(value));
    try {
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

  constructor(executeCommand, queryState, render) {
    this.#executeCommand = executeCommand;
    this.#queryState = queryState;
    this.#render = render;
  }

  execute(command) {
    return this.#executeCommand(command);
  }

  query(query) {
    return this.#queryState(query);
  }

  renderFrame(aspect, view = OVERVIEW_VIEW) {
    return this.#render(aspect, view);
  }
}

function requireSuccess(response) {
  if (response?.ok !== true) {
    throw new Error(response?.error ?? "city-game WASM operation failed");
  }
  return response;
}
