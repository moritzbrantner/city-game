//! Authoritative scenario, simulation-state, progression, and render-adapter core for city-game.
//!
//! OSM parsing stays in `moenarch-geo-io-osm`; generic camera/renderer mechanics stay in 3d-lab.

mod model;
mod osm;
mod progression;
mod render;

pub use model::{
    CitySave, CityScenario, CityWorld, ExternalRevision, ScenarioFeature, ScenarioFeatureKind,
    ScenarioProvenance, SAVE_SCHEMA_VERSION, SCENARIO_SCHEMA_VERSION,
};
pub use osm::{
    import_osm_pbf_bytes, GEO_ANALYSIS_REVISION, OSM_PARSER_REPOSITORY,
};
pub use progression::{
    ProgressionRule, ProgressionState, Requirement,
};
pub use render::{
    build_render_frame, RendererCamera, RendererFrame, RendererGeometry, RendererSceneNode,
    RendererTransform, THREE_D_LAB_REVISION,
};
