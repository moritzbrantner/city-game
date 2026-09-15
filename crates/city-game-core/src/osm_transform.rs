use std::{collections::BTreeSet, fmt};

use serde::{Deserialize, Serialize};

use crate::{CityScenario, RoadClass};

pub const OSM_SCENARIO_TRANSFORM_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OsmFeatureTransformPolicy {
    Preserve,
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OsmUnmappedRoadPolicy {
    Preserve,
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OsmRoadClassMapping {
    pub sources: Vec<RoadClass>,
    pub target: RoadClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OsmRoadTransformConfig {
    pub mappings: Vec<OsmRoadClassMapping>,
    pub unmapped: OsmUnmappedRoadPolicy,
    pub preserve_names: bool,
    pub preserve_lanes: bool,
    pub preserve_max_speed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OsmScenarioTransformConfig {
    pub schema_version: u32,
    pub roads: OsmRoadTransformConfig,
    pub buildings: OsmFeatureTransformPolicy,
    pub water: OsmFeatureTransformPolicy,
    pub land_use: OsmFeatureTransformPolicy,
    pub transit: OsmFeatureTransformPolicy,
}

impl Default for OsmScenarioTransformConfig {
    fn default() -> Self {
        Self::preserve_all()
    }
}

impl OsmScenarioTransformConfig {
    #[must_use]
    pub fn preserve_all() -> Self {
        Self {
            schema_version: OSM_SCENARIO_TRANSFORM_SCHEMA_VERSION,
            roads: OsmRoadTransformConfig {
                mappings: Vec::new(),
                unmapped: OsmUnmappedRoadPolicy::Preserve,
                preserve_names: true,
                preserve_lanes: true,
                preserve_max_speed: true,
            },
            buildings: OsmFeatureTransformPolicy::Preserve,
            water: OsmFeatureTransformPolicy::Preserve,
            land_use: OsmFeatureTransformPolicy::Preserve,
            transit: OsmFeatureTransformPolicy::Preserve,
        }
    }

    /// Initial game-oriented OSM import profile: keep only the street layout and collapse the
    /// parser-facing road taxonomy into three game road classes.
    #[must_use]
    pub fn road_layout_only() -> Self {
        Self {
            schema_version: OSM_SCENARIO_TRANSFORM_SCHEMA_VERSION,
            roads: OsmRoadTransformConfig {
                mappings: vec![
                    OsmRoadClassMapping {
                        sources: vec![RoadClass::Motorway, RoadClass::Trunk, RoadClass::Primary],
                        target: RoadClass::Primary,
                    },
                    OsmRoadClassMapping {
                        sources: vec![RoadClass::Secondary, RoadClass::Tertiary],
                        target: RoadClass::Secondary,
                    },
                    OsmRoadClassMapping {
                        sources: vec![RoadClass::Residential, RoadClass::Service],
                        target: RoadClass::Residential,
                    },
                ],
                unmapped: OsmUnmappedRoadPolicy::Ignore,
                preserve_names: false,
                preserve_lanes: false,
                preserve_max_speed: false,
            },
            buildings: OsmFeatureTransformPolicy::Ignore,
            water: OsmFeatureTransformPolicy::Ignore,
            land_use: OsmFeatureTransformPolicy::Ignore,
            transit: OsmFeatureTransformPolicy::Ignore,
        }
    }

    pub fn validate(&self) -> Result<(), OsmScenarioTransformError> {
        if self.schema_version != OSM_SCENARIO_TRANSFORM_SCHEMA_VERSION {
            return Err(OsmScenarioTransformError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }

        let mut mapped_sources = BTreeSet::new();
        for (index, mapping) in self.roads.mappings.iter().enumerate() {
            if mapping.sources.is_empty() {
                return Err(OsmScenarioTransformError::EmptyRoadMapping(index));
            }
            for source in &mapping.sources {
                if !mapped_sources.insert(*source) {
                    return Err(OsmScenarioTransformError::DuplicateRoadSource(*source));
                }
            }
        }

        Ok(())
    }

    pub fn transform(
        &self,
        mut scenario: CityScenario,
    ) -> Result<CityScenario, OsmScenarioTransformError> {
        self.validate()?;

        scenario.roads = scenario
            .roads
            .into_iter()
            .filter_map(|mut road| {
                let mapped = self
                    .roads
                    .mappings
                    .iter()
                    .find(|mapping| mapping.sources.contains(&road.class));

                match mapped {
                    Some(mapping) => road.class = mapping.target,
                    None if self.roads.unmapped == OsmUnmappedRoadPolicy::Preserve => {}
                    None => return None,
                }

                if !self.roads.preserve_names {
                    road.name = None;
                }
                if !self.roads.preserve_lanes {
                    road.lanes = None;
                }
                if !self.roads.preserve_max_speed {
                    road.max_speed_kph = None;
                }
                Some(road)
            })
            .collect();

        if self.buildings == OsmFeatureTransformPolicy::Ignore {
            scenario.buildings.clear();
        }
        if self.water == OsmFeatureTransformPolicy::Ignore {
            scenario.water.clear();
        }
        if self.land_use == OsmFeatureTransformPolicy::Ignore {
            scenario.land_use_areas.clear();
        }
        if self.transit == OsmFeatureTransformPolicy::Ignore {
            scenario.transit_anchors.clear();
        }

        Ok(scenario)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsmScenarioTransformError {
    UnsupportedSchemaVersion(u32),
    EmptyRoadMapping(usize),
    DuplicateRoadSource(RoadClass),
}

impl fmt::Display for OsmScenarioTransformError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion(version) => write!(
                formatter,
                "unsupported OSM scenario transform schema version {version}; expected {OSM_SCENARIO_TRANSFORM_SCHEMA_VERSION}"
            ),
            Self::EmptyRoadMapping(index) => {
                write!(
                    formatter,
                    "OSM road mapping {index} has no source road classes"
                )
            }
            Self::DuplicateRoadSource(source) => {
                write!(
                    formatter,
                    "OSM road class {source:?} is mapped more than once"
                )
            }
        }
    }
}

impl std::error::Error for OsmScenarioTransformError {}

#[cfg(test)]
mod tests {
    use geo_core::Geometry;

    use crate::{
        BuildingUse, ExternalRevision, LandUseKind, SCENARIO_SCHEMA_VERSION, ScenarioBuilding,
        ScenarioLandUse, ScenarioProvenance, ScenarioRoad, ScenarioTransitAnchor, ScenarioWater,
        TransitKind, WaterKind,
    };

    use super::*;

    fn line(offset: f64) -> Geometry {
        Geometry::LineString {
            coordinates: vec![[8.0 + offset, 48.0], [8.001 + offset, 48.001]],
        }
    }

    fn road(id: &str, class: RoadClass) -> ScenarioRoad {
        ScenarioRoad {
            id: id.to_owned(),
            source_id: id.to_owned(),
            geometry: line(0.0),
            class,
            name: Some("source name".to_owned()),
            lanes: Some(4),
            max_speed_kph: Some(70),
        }
    }

    fn scenario() -> CityScenario {
        CityScenario {
            schema_version: SCENARIO_SCHEMA_VERSION,
            provenance: ScenarioProvenance {
                source_format: "osm-pbf".to_owned(),
                source_name: "transform-fixture.osm.pbf".to_owned(),
                source_sha256: "fixture".to_owned(),
                parser: ExternalRevision {
                    repository: "fixture".to_owned(),
                    revision: "fixture".to_owned(),
                },
            },
            roads: vec![
                road("motorway", RoadClass::Motorway),
                road("secondary", RoadClass::Secondary),
                road("local", RoadClass::Residential),
                road("footway", RoadClass::Footway),
            ],
            buildings: vec![ScenarioBuilding {
                id: "building".to_owned(),
                source_id: "building".to_owned(),
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
                height_m: None,
                gross_floor_area_m2: 1,
            }],
            water: vec![ScenarioWater {
                id: "water".to_owned(),
                source_id: "water".to_owned(),
                geometry: line(0.0),
                kind: WaterKind::River,
            }],
            land_use_areas: vec![ScenarioLandUse {
                id: "land".to_owned(),
                source_id: "land".to_owned(),
                geometry: line(0.0),
                kind: LandUseKind::Residential,
            }],
            transit_anchors: vec![ScenarioTransitAnchor {
                id: "transit".to_owned(),
                source_id: "transit".to_owned(),
                geometry: line(0.0),
                kind: TransitKind::BusStop,
                name: None,
            }],
        }
    }

    #[test]
    fn road_layout_profile_emits_only_three_game_road_classes_and_ignores_other_features() {
        let transformed = OsmScenarioTransformConfig::road_layout_only()
            .transform(scenario())
            .unwrap();

        assert_eq!(transformed.roads.len(), 3);
        assert_eq!(transformed.roads[0].class, RoadClass::Primary);
        assert_eq!(transformed.roads[1].class, RoadClass::Secondary);
        assert_eq!(transformed.roads[2].class, RoadClass::Residential);
        assert!(transformed.roads.iter().all(|road| road.name.is_none()));
        assert!(transformed.roads.iter().all(|road| road.lanes.is_none()));
        assert!(
            transformed
                .roads
                .iter()
                .all(|road| road.max_speed_kph.is_none())
        );
        assert!(transformed.buildings.is_empty());
        assert!(transformed.water.is_empty());
        assert!(transformed.land_use_areas.is_empty());
        assert!(transformed.transit_anchors.is_empty());
    }

    #[test]
    fn preserve_all_profile_is_identity() {
        let original = scenario();
        let transformed = OsmScenarioTransformConfig::preserve_all()
            .transform(original.clone())
            .unwrap();

        assert_eq!(transformed, original);
    }

    #[test]
    fn duplicate_source_road_mapping_fails_closed() {
        let mut config = OsmScenarioTransformConfig::road_layout_only();
        config.roads.mappings.push(OsmRoadClassMapping {
            sources: vec![RoadClass::Primary],
            target: RoadClass::Residential,
        });

        assert_eq!(
            config.validate(),
            Err(OsmScenarioTransformError::DuplicateRoadSource(
                RoadClass::Primary
            ))
        );
    }
}
