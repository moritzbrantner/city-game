//! Authoritative scenario, simulation-state, progression, and render-adapter core for city-game.
//!
//! OSM parsing stays in `moenarch-geo-io-osm`; generic camera/renderer mechanics stay in 3d-lab.

mod model;
mod osm;
mod progression;
mod render;

pub use model::{
    CitySave, CityScenario, CityWorld, ExternalRevision, SAVE_SCHEMA_VERSION,
    SCENARIO_SCHEMA_VERSION, ScenarioFeature, ScenarioFeatureKind, ScenarioProvenance,
};
pub use osm::{GEO_ANALYSIS_REVISION, OSM_PARSER_REPOSITORY, import_osm_pbf_bytes};
pub use progression::{ProgressionRule, ProgressionState, Requirement};
pub use render::{
    RendererCamera, RendererFrame, RendererGeometry, RendererSceneNode, RendererTransform,
    THREE_D_LAB_REVISION, build_render_frame,
};
