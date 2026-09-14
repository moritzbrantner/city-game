use std::collections::BTreeMap;

use geo_core::Geometry;
use serde::{Deserialize, Serialize};

use crate::{
    CityPlanningOverlay, CityTimeConfig, PopulationRules, PopulationState, ProgressionState,
};

pub const SCENARIO_SCHEMA_VERSION: u32 = 3;
pub const SAVE_SCHEMA_VERSION: u32 = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRevision {
    pub repository: String,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioProvenance {
    pub source_format: String,
    pub source_name: String,
    pub source_sha256: String,
    pub parser: ExternalRevision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoadClass {
    Motorway,
    Trunk,
    Primary,
    Secondary,
    Tertiary,
    Residential,
    Service,
    Track,
    Pedestrian,
    Cycleway,
    Footway,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BuildingUse {
    Residential,
    Commercial,
    Industrial,
    Civic,
    Agricultural,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WaterKind {
    Body,
    River,
    Stream,
    Canal,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LandUseKind {
    Residential,
    Commercial,
    Industrial,
    Retail,
    Forest,
    Farmland,
    Recreation,
    Cemetery,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransitKind {
    BusStop,
    Platform,
    Station,
    TramStop,
    StopPosition,
    Rail,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioRoad {
    pub id: String,
    pub source_id: String,
    pub geometry: Geometry,
    pub class: RoadClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lanes: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_speed_kph: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioBuilding {
    pub id: String,
    pub source_id: String,
    pub footprint: Geometry,
    pub use_kind: BuildingUse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub levels: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height_m: Option<f32>,
    pub gross_floor_area_m2: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioWater {
    pub id: String,
    pub source_id: String,
    pub geometry: Geometry,
    pub kind: WaterKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioLandUse {
    pub id: String,
    pub source_id: String,
    pub geometry: Geometry,
    pub kind: LandUseKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioTransitAnchor {
    pub id: String,
    pub source_id: String,
    pub geometry: Geometry,
    pub kind: TransitKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CityScenario {
    pub schema_version: u32,
    pub provenance: ScenarioProvenance,
    pub roads: Vec<ScenarioRoad>,
    pub buildings: Vec<ScenarioBuilding>,
    pub water: Vec<ScenarioWater>,
    pub land_use_areas: Vec<ScenarioLandUse>,
    pub transit_anchors: Vec<ScenarioTransitAnchor>,
}

impl CityScenario {
    pub fn contains_entity(&self, id: &str) -> bool {
        self.roads.iter().any(|entity| entity.id == id)
            || self.buildings.iter().any(|entity| entity.id == id)
            || self.water.iter().any(|entity| entity.id == id)
            || self.land_use_areas.iter().any(|entity| entity.id == id)
            || self.transit_anchors.iter().any(|entity| entity.id == id)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CityWorld {
    pub tick: u64,
    pub metrics: BTreeMap<String, i64>,
    pub progression: ProgressionState,
    pub planning: CityPlanningOverlay,
    pub population: PopulationState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitySave {
    pub schema_version: u32,
    pub scenario: CityScenario,
    pub time: CityTimeConfig,
    pub population_rules: PopulationRules,
    pub world: CityWorld,
}

impl CitySave {
    pub fn new(scenario: CityScenario) -> Self {
        let population_rules = PopulationRules::default();
        let population = PopulationState::baseline_from_scenario(&scenario, population_rules)
            .expect("canonical scenario capacity fits aggregate population state");
        Self {
            schema_version: SAVE_SCHEMA_VERSION,
            scenario,
            time: CityTimeConfig::default(),
            population_rules,
            world: CityWorld {
                population,
                ..CityWorld::default()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenario() -> CityScenario {
        CityScenario {
            schema_version: SCENARIO_SCHEMA_VERSION,
            provenance: ScenarioProvenance {
                source_format: "osm-pbf".to_owned(),
                source_name: "fixture.osm.pbf".to_owned(),
                source_sha256: "abc123".to_owned(),
                parser: ExternalRevision {
                    repository: "geo-analysis".to_owned(),
                    revision: "deadbeef".to_owned(),
                },
            },
            roads: Vec::new(),
            buildings: Vec::new(),
            water: Vec::new(),
            land_use_areas: Vec::new(),
            transit_anchors: Vec::new(),
        }
    }

    #[test]
    fn save_roundtrip_preserves_game_scenario_provenance_clock_and_population_rules() {
        let mut save = CitySave::new(scenario());
        save.advance_tick().unwrap();

        let encoded = serde_json::to_string(&save).unwrap();
        let decoded: CitySave = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, save);
        assert_eq!(decoded.scenario.provenance.source_sha256, "abc123");
        assert_eq!(decoded.world.tick, 1);
        assert_eq!(decoded.time, CityTimeConfig::default());
        assert_eq!(decoded.population_rules, PopulationRules::default());
        assert_eq!(decoded.time_position().unwrap().minute_of_day, 15);
        assert!(!encoded.contains("\"tags\""));
    }
}
