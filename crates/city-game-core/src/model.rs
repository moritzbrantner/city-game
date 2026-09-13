use std::collections::BTreeMap;

use geo_core::Geometry;
use serde::{Deserialize, Serialize};

use crate::ProgressionState;

pub const SCENARIO_SCHEMA_VERSION: u32 = 1;
pub const SAVE_SCHEMA_VERSION: u32 = 1;

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
pub enum ScenarioFeatureKind {
    Road,
    Building,
    Water,
    LandUse,
    Transit,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioFeature {
    pub source_id: String,
    pub kind: ScenarioFeatureKind,
    pub tags: BTreeMap<String, String>,
    pub geometry: Geometry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CityScenario {
    pub schema_version: u32,
    pub provenance: ScenarioProvenance,
    pub features: Vec<ScenarioFeature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CityWorld {
    pub tick: u64,
    pub metrics: BTreeMap<String, i64>,
    pub progression: ProgressionState,
}

impl Default for CityWorld {
    fn default() -> Self {
        Self {
            tick: 0,
            metrics: BTreeMap::new(),
            progression: ProgressionState::default(),
        }
    }
}

impl CityWorld {
    pub fn advance_tick(&mut self) {
        self.tick = self.tick.saturating_add(1);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitySave {
    pub schema_version: u32,
    pub scenario: CityScenario,
    pub world: CityWorld,
}

impl CitySave {
    pub fn new(scenario: CityScenario) -> Self {
        Self {
            schema_version: SAVE_SCHEMA_VERSION,
            scenario,
            world: CityWorld::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_roundtrip_preserves_source_provenance() {
        let scenario = CityScenario {
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
            features: Vec::new(),
        };
        let mut save = CitySave::new(scenario);
        save.world.advance_tick();

        let encoded = serde_json::to_string(&save).unwrap();
        let decoded: CitySave = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, save);
        assert_eq!(decoded.scenario.provenance.source_sha256, "abc123");
        assert_eq!(decoded.world.tick, 1);
    }
}
