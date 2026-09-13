import { createThreeSceneRenderer, validateRenderFrame } from "@moritzbrantner/three-d-renderer";

const canvas = document.querySelector("#scene");
const scenarioLabel = document.querySelector("#scenario");
const objectsLabel = document.querySelector("#objects");

if (!(canvas instanceof HTMLCanvasElement)) {
  throw new Error("city-game foundation canvas is missing");
}

const response = await fetch("./demo-frame.json");
if (!response.ok) {
  throw new Error(`failed to load generated renderer frame: ${response.status}`);
}
const frame = validateRenderFrame(await response.json());
const renderer = createThreeSceneRenderer(canvas, {
  antialias: true,
  background: 0xe5e4de,
  pixelRatioLimit: 2,
  shadows: false,
});

scenarioLabel.textContent = "synthetic OSM consumer fixture";
objectsLabel.textContent = String(frame.nodes.length);

function render() {
  const width = Math.max(1, canvas.clientWidth);
  const height = Math.max(1, canvas.clientHeight);
  renderer.setSize(width, height, window.devicePixelRatio);
  renderer.render(frame);
}

const observer = new ResizeObserver(render);
observer.observe(canvas);
window.addEventListener("pagehide", () => {
  observer.disconnect();
  renderer.dispose();
});
render();
