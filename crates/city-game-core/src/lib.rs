//! Authoritative scenario, simulation-state, progression, planning, and render-adapter core for city-game.
//!
//! OSM parsing stays in `moenarch-geo-io-osm`; generic camera/renderer mechanics stay in 3d-lab.
//! OSM is an import format only: simulation and persistence operate on the canonical game schema.

mod model;
mod osm;
mod planning;
mod progression;
mod render;

pub use model::{
    BuildingUse, CitySave, CityScenario, CityWorld, ExternalRevision, LandUseKind, RoadClass,
    SAVE_SCHEMA_VERSION, SCENARIO_SCHEMA_VERSION, ScenarioBuilding, ScenarioLandUse,
    ScenarioProvenance, ScenarioRoad, ScenarioTransitAnchor, ScenarioWater, TransitKind, WaterKind,
};
pub use osm::{GEO_ANALYSIS_REVISION, OSM_PARSER_REPOSITORY, import_osm_pbf_bytes};
pub use planning::{
    CityPlanningOverlay, EffectiveRoad, EffectiveRoadOrigin, PlannedRoad, PlannedZone, PlanningCommand,
    PlanningError, PlanningOutcome, ZoneKind,
};
pub use progression::{ProgressionRule, ProgressionState, Requirement};
pub use render::{
    RendererCamera, RendererFrame, RendererGeometry, RendererSceneNode, RendererTransform,
    THREE_D_LAB_REVISION, build_render_frame,
};
