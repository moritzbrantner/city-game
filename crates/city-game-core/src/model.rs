use std::{collections::BTreeMap, fmt};

use geo_core::Geometry;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{
    CityPlanningOverlay, CityRuleset, CityTimeConfig, CityTimeError, PopulationError,
    PopulationState, ProgressionState, RuleSystem, RulesetError,
};

pub const SCENARIO_SCHEMA_VERSION: u32 = 3;
pub const SAVE_SCHEMA_VERSION: u32 = 6;

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
    pub fn validate_schema(&self) -> Result<(), CityScenarioError> {
        if self.schema_version == SCENARIO_SCHEMA_VERSION {
            Ok(())
        } else {
            Err(CityScenarioError::UnsupportedSchemaVersion(
                self.schema_version,
            ))
        }
    }

    pub fn contains_entity(&self, id: &str) -> bool {
        self.roads.iter().any(|entity| entity.id == id)
            || self.buildings.iter().any(|entity| entity.id == id)
            || self.water.iter().any(|entity| entity.id == id)
            || self.land_use_areas.iter().any(|entity| entity.id == id)
            || self.transit_anchors.iter().any(|entity| entity.id == id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CityScenarioError {
    UnsupportedSchemaVersion(u32),
}

impl fmt::Display for CityScenarioError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion(version) => write!(
                formatter,
                "unsupported city scenario schema version {version}; expected {SCENARIO_SCHEMA_VERSION}"
            ),
        }
    }
}

impl std::error::Error for CityScenarioError {}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CityWorld {
    pub tick: u64,
    pub metrics: BTreeMap<String, i64>,
    pub progression: ProgressionState,
    pub planning: CityPlanningOverlay,
    pub population: PopulationState,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitySave {
    pub(crate) schema_version: u32,
    pub(crate) scenario: CityScenario,
    pub(crate) time: CityTimeConfig,
    pub(crate) ruleset: CityRuleset,
    pub(crate) world: CityWorld,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CitySaveError {
    Scenario(CityScenarioError),
    Ruleset(RulesetError),
    Time(CityTimeError),
    Population(PopulationError),
}

impl fmt::Display for CitySaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scenario(error) => error.fmt(formatter),
            Self::Ruleset(error) => error.fmt(formatter),
            Self::Time(error) => error.fmt(formatter),
            Self::Population(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for CitySaveError {}

impl From<CityScenarioError> for CitySaveError {
    fn from(error: CityScenarioError) -> Self {
        Self::Scenario(error)
    }
}

impl From<RulesetError> for CitySaveError {
    fn from(error: RulesetError) -> Self {
        Self::Ruleset(error)
    }
}

impl From<CityTimeError> for CitySaveError {
    fn from(error: CityTimeError) -> Self {
        Self::Time(error)
    }
}

impl From<PopulationError> for CitySaveError {
    fn from(error: PopulationError) -> Self {
        Self::Population(error)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CitySaveWire {
    schema_version: u32,
    scenario: CityScenario,
    time: CityTimeConfig,
    ruleset: CityRuleset,
    world: CityWorld,
}

impl<'de> Deserialize<'de> for CitySave {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CitySaveWire::deserialize(deserializer)?;
        if wire.schema_version != SAVE_SCHEMA_VERSION {
            return Err(D::Error::custom(format!(
                "unsupported city save schema version {}; expected {SAVE_SCHEMA_VERSION}",
                wire.schema_version
            )));
        }
        wire.world
            .planning
            .validate_against_scenario(&wire.scenario)
            .map_err(D::Error::custom)?;

        let mut save = Self::new_with_config(wire.scenario, wire.time, wire.ruleset)
            .map_err(D::Error::custom)?;
        save.world = wire.world;
        Ok(save)
    }
}

impl CitySave {
    pub fn new(scenario: CityScenario) -> Result<Self, CitySaveError> {
        Self::new_with_config(scenario, CityTimeConfig::default(), CityRuleset::default())
    }

    pub fn new_with_ruleset(
        scenario: CityScenario,
        ruleset: CityRuleset,
    ) -> Result<Self, CitySaveError> {
        Self::new_with_config(scenario, CityTimeConfig::default(), ruleset)
    }

    pub fn new_with_config(
        scenario: CityScenario,
        time: CityTimeConfig,
        ruleset: CityRuleset,
    ) -> Result<Self, CitySaveError> {
        scenario.validate_schema()?;
        time.validate()?;
        ruleset.validate()?;

        let population = if ruleset.is_enabled(RuleSystem::Population) {
            PopulationState::baseline_from_scenario(&scenario, ruleset.population.config)?
        } else {
            PopulationState::default()
        };

        Ok(Self {
            schema_version: SAVE_SCHEMA_VERSION,
            scenario,
            time,
            ruleset,
            world: CityWorld {
                population,
                ..CityWorld::default()
            },
        })
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

    fn residential_building(id: &str, gross_floor_area_m2: u64) -> ScenarioBuilding {
        ScenarioBuilding {
            id: id.to_owned(),
            source_id: id.to_owned(),
            footprint: Geometry::Polygon {
                coordinates: vec![vec![
                    [8.0, 48.0],
                    [8.001, 48.0],
                    [8.001, 48.001],
                    [8.0, 48.0],
                ]],
            },
            use_kind: BuildingUse::Residential,
            name: None,
            levels: Some(1),
            height_m: Some(3.0),
            gross_floor_area_m2,
        }
    }

    #[test]
    fn save_roundtrip_preserves_current_scenario_clock_and_ruleset() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.execute(crate::CityCommand::AdvanceFixedSteps { steps: 1 })
            .unwrap();

        let encoded = serde_json::to_string(&save).unwrap();
        let decoded: CitySave = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, save);
        assert_eq!(decoded.schema_version, SAVE_SCHEMA_VERSION);
        assert_eq!(decoded.scenario.provenance.source_sha256, "abc123");
        assert_eq!(decoded.world.tick, 1);
        assert_eq!(decoded.time, CityTimeConfig::default());
        assert_eq!(decoded.ruleset, CityRuleset::default());
        assert_eq!(decoded.time_position().unwrap().minute_of_day, 15);
        assert!(!encoded.contains("\"tags\""));
    }

    #[test]
    fn non_current_save_schema_fails_instead_of_migrating() {
        let save = CitySave::new(scenario()).unwrap();
        let mut encoded = serde_json::to_value(save).unwrap();
        encoded["schemaVersion"] = serde_json::json!(SAVE_SCHEMA_VERSION - 1);

        let error = serde_json::from_value::<CitySave>(encoded).unwrap_err();

        assert!(error.to_string().contains("unsupported city save schema"));
    }

    #[test]
    fn non_current_scenario_schema_fails_instead_of_migrating() {
        let mut invalid = scenario();
        invalid.schema_version = SCENARIO_SCHEMA_VERSION - 1;

        assert!(matches!(
            CitySave::new(invalid),
            Err(CitySaveError::Scenario(
                CityScenarioError::UnsupportedSchemaVersion(_)
            ))
        ));
    }

    #[test]
    fn custom_typed_rules_are_applied_when_a_save_is_created() {
        let mut ruleset = CityRuleset::default();
        ruleset
            .population
            .config
            .residential_floor_area_m2_per_household = 72;

        let save = CitySave::new_with_ruleset(scenario(), ruleset.clone()).unwrap();

        assert_eq!(save.ruleset, ruleset);
    }

    #[test]
    fn current_save_load_rejects_population_capacity_overflow() {
        let mut overflowing_scenario = scenario();
        overflowing_scenario.buildings = vec![
            residential_building("residential/1", u64::MAX),
            residential_building("residential/2", u64::MAX),
        ];
        let mut ruleset = CityRuleset::default();
        ruleset
            .population
            .config
            .residential_floor_area_m2_per_household = 1;
        let invalid = CitySave {
            schema_version: SAVE_SCHEMA_VERSION,
            scenario: overflowing_scenario,
            time: CityTimeConfig::default(),
            ruleset,
            world: CityWorld::default(),
        };

        let encoded = serde_json::to_string(&invalid).unwrap();
        let error = serde_json::from_str::<CitySave>(&encoded).unwrap_err();

        assert!(error.to_string().contains("population capacity overflow"));
    }
}
