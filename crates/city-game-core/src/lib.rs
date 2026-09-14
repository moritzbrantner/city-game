//! Authoritative scenario, simulation-state, progression, planning, population, time, and render-adapter core for city-game.
//!
//! OSM parsing stays in `moenarch-geo-io-osm`; generic camera/renderer mechanics stay in 3d-lab.
//! OSM is an import format only: simulation and persistence operate on the canonical game schema.
//! Application writes should enter through [`CitySave::execute`]; reads can use [`CitySave::queries`].

mod commands;
mod model;
mod osm;
mod planning;
mod population;
mod progression;
mod queries;
mod render;
mod rules;
mod time;

pub use commands::{CityCommand, CityCommandError, CityCommandOutcome};
pub use model::{
    BuildingUse, CitySave, CitySaveError, CityScenario, CityWorld, ExternalRevision, LandUseKind,
    RoadClass, SAVE_SCHEMA_VERSION, SCENARIO_SCHEMA_VERSION, ScenarioBuilding, ScenarioLandUse,
    ScenarioProvenance, ScenarioRoad, ScenarioTransitAnchor, ScenarioWater, TransitKind, WaterKind,
};
pub use osm::{GEO_ANALYSIS_REVISION, OSM_PARSER_REPOSITORY, import_osm_pbf_bytes};
pub use planning::{
    CityPlanningOverlay, EffectiveRoad, EffectiveRoadOrigin, PlannedRoad, PlannedZone,
    PlanningCommand, PlanningError, PlanningOutcome, ZoneKind,
};
pub use population::{
    PopulationCapacity, PopulationError, PopulationRules, PopulationState, RciDemand,
};
pub use progression::{ProgressionRule, ProgressionState, Requirement};
pub use queries::{CityQueries, CityQueryError};
pub use render::{
    RendererCamera, RendererFrame, RendererGeometry, RendererSceneNode, RendererTransform,
    THREE_D_LAB_REVISION, build_render_frame, build_save_render_frame,
};
pub use rules::{CityRuleset, RULESET_SCHEMA_VERSION, RuleStatus, RuleSystem, RulesetError};
pub use time::{
    CityTimeConfig, CityTimeError, CityTimePosition, DEFAULT_MINUTES_PER_TICK, MINUTES_PER_DAY,
};
