const baseUrl = new URL(document.baseURI);
const basePath = applicationBasePath(baseUrl.pathname);
const scenarioRoutePrefix = `${basePath}scenarios/`;
const scenarioIdPattern = /^[a-z0-9-]+$/;
let canonicalizationQueued = false;

function applicationBasePath(pathname) {
  if (pathname.endsWith("/")) return pathname;
  const separator = pathname.lastIndexOf("/");
  return separator >= 0 ? pathname.slice(0, separator + 1) : "/";
}

function scenarioIdFromPath(pathname) {
  if (!pathname.startsWith(scenarioRoutePrefix)) return null;
  const remainder = pathname.slice(scenarioRoutePrefix.length);
  const segments = remainder.split("/").filter(Boolean);
  if (segments.length !== 1) return null;
  const scenarioId = decodeURIComponent(segments[0]);
  return scenarioIdPattern.test(scenarioId) ? scenarioId : null;
}

function queueCanonicalization() {
  if (canonicalizationQueued) return;
  canonicalizationQueued = true;
  queueMicrotask(() => {
    canonicalizationQueued = false;
    canonicalizeLocation();
  });
}

function canonicalizeLocation() {
  const url = new URL(window.location.href);
  const queryScenarioId = url.searchParams.get("scenario");
  const routeScenarioId = scenarioIdFromPath(url.pathname);
  const scenarioId = scenarioIdPattern.test(queryScenarioId ?? "") ? queryScenarioId : routeScenarioId;

  if (document.body.dataset.mode === "city" && scenarioId) {
    const targetPath = `${scenarioRoutePrefix}${encodeURIComponent(scenarioId)}/`;
    url.pathname = targetPath;
    url.searchParams.delete("scenario");
    url.searchParams.delete("mode");
    replaceIfChanged(url);
    return;
  }

  if (document.body.dataset.mode === "picker" && routeScenarioId) {
    url.pathname = basePath;
    url.searchParams.set("scenario", scenarioId ?? routeScenarioId);
    url.searchParams.delete("mode");
    replaceIfChanged(url);
  }
}

function replaceIfChanged(url) {
  const next = `${url.pathname}${url.search}${url.hash}`;
  const current = `${window.location.pathname}${window.location.search}${window.location.hash}`;
  if (next !== current) history.replaceState(history.state, "", next);
}

new MutationObserver(queueCanonicalization).observe(document.body, {
  attributes: true,
  attributeFilter: ["data-mode"],
});
