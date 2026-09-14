use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use geo_core::Geometry;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{CitySave, CityScenario, CityWorld, PopulationError, RoadClass};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ZoneKind {
    Residential,
    Commercial,
    Industrial,
    MixedUse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedRoad {
    pub id: String,
    pub geometry: Geometry,
    pub class: RoadClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedZone {
    pub id: String,
    pub geometry: Geometry,
    pub kind: ZoneKind,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CityPlanningOverlay {
    pub player_roads: BTreeMap<String, PlannedRoad>,
    pub zones: BTreeMap<String, PlannedZone>,
    pub suppressed_scenario_entities: BTreeSet<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CityPlanningOverlayWire {
    player_roads: BTreeMap<String, PlannedRoad>,
    zones: BTreeMap<String, PlannedZone>,
    suppressed_scenario_entities: BTreeSet<String>,
}

impl<'de> Deserialize<'de> for CityPlanningOverlay {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CityPlanningOverlayWire::deserialize(deserializer)?;
        let overlay = Self {
            player_roads: wire.player_roads,
            zones: wire.zones,
            suppressed_scenario_entities: wire.suppressed_scenario_entities,
        };
        overlay.validate_loaded().map_err(D::Error::custom)?;
        Ok(overlay)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PlanningCommand {
    AddRoad { road: PlannedRoad },
    RemovePlayerRoad { id: String },
    SuppressScenarioEntity { id: String },
    RestoreScenarioEntity { id: String },
    ZoneArea { zone: PlannedZone },
    RemoveZone { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanningOutcome {
    Applied,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanningError {
    InvalidId,
    InvalidRoadGeometry,
    InvalidZoneGeometry,
    MismatchedPlayerEntityKey { key: String, id: String },
    ConflictingPlayerEntityId(String),
    ScenarioEntityIdReserved(String),
    UnknownScenarioEntity(String),
}

impl fmt::Display for PlanningError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId => formatter.write_str("planning entity id must be non-empty"),
            Self::InvalidRoadGeometry => {
                formatter.write_str("player road geometry must contain at least one line segment")
            }
            Self::InvalidZoneGeometry => {
                formatter.write_str("zone geometry must be a polygon or multipolygon")
            }
            Self::MismatchedPlayerEntityKey { key, id } => write!(
                formatter,
                "persisted player entity map key {key} does not match nested id {id}"
            ),
            Self::ConflictingPlayerEntityId(id) => {
                write!(
                    formatter,
                    "player entity id {id} is already used by different state"
                )
            }
            Self::ScenarioEntityIdReserved(id) => {
                write!(
                    formatter,
                    "player entity id {id} is reserved by the imported scenario"
                )
            }
            Self::UnknownScenarioEntity(id) => {
                write!(formatter, "scenario entity {id} does not exist")
            }
        }
    }
}

impl std::error::Error for PlanningError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectiveRoadOrigin {
    Scenario,
    Player,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveRoad {
    pub id: String,
    pub geometry: Geometry,
    pub class: RoadClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub origin: EffectiveRoadOrigin,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
}

impl CityPlanningOverlay {
    pub fn apply(
        &mut self,
        scenario: &CityScenario,
        command: PlanningCommand,
    ) -> Result<PlanningOutcome, PlanningError> {
        match command {
            PlanningCommand::AddRoad { road } => self.add_road(scenario, road),
            PlanningCommand::RemovePlayerRoad { id } => {
                validate_id(&id)?;
                Ok(if self.player_roads.remove(&id).is_some() {
                    PlanningOutcome::Applied
                } else {
                    PlanningOutcome::Unchanged
                })
            }
            PlanningCommand::SuppressScenarioEntity { id } => {
                validate_id(&id)?;
                if !scenario.contains_entity(&id) {
                    return Err(PlanningError::UnknownScenarioEntity(id));
                }
                Ok(if self.suppressed_scenario_entities.insert(id) {
                    PlanningOutcome::Applied
                } else {
                    PlanningOutcome::Unchanged
                })
            }
            PlanningCommand::RestoreScenarioEntity { id } => {
                validate_id(&id)?;
                if !scenario.contains_entity(&id) {
                    return Err(PlanningError::UnknownScenarioEntity(id));
                }
                Ok(if self.suppressed_scenario_entities.remove(&id) {
                    PlanningOutcome::Applied
                } else {
                    PlanningOutcome::Unchanged
                })
            }
            PlanningCommand::ZoneArea { zone } => self.zone_area(scenario, zone),
            PlanningCommand::RemoveZone { id } => {
                validate_id(&id)?;
                Ok(if self.zones.remove(&id).is_some() {
                    PlanningOutcome::Applied
                } else {
                    PlanningOutcome::Unchanged
                })
            }
        }
    }

    pub fn is_suppressed(&self, scenario_entity_id: &str) -> bool {
        self.suppressed_scenario_entities
            .contains(scenario_entity_id)
    }

    pub fn effective_roads(&self, scenario: &CityScenario) -> Vec<EffectiveRoad> {
        let mut roads = scenario
            .roads
            .iter()
            .filter(|road| !self.is_suppressed(&road.id))
            .map(|road| EffectiveRoad {
                id: road.id.clone(),
                geometry: road.geometry.clone(),
                class: road.class,
                name: road.name.clone(),
                origin: EffectiveRoadOrigin::Scenario,
                source_id: Some(road.source_id.clone()),
            })
            .collect::<Vec<_>>();

        roads.extend(self.player_roads.values().map(|road| EffectiveRoad {
            id: road.id.clone(),
            geometry: road.geometry.clone(),
            class: road.class,
            name: road.name.clone(),
            origin: EffectiveRoadOrigin::Player,
            source_id: None,
        }));
        roads.sort_by(|left, right| left.id.cmp(&right.id));
        roads
    }

    pub(crate) fn validate_against_scenario(
        &self,
        scenario: &CityScenario,
    ) -> Result<(), PlanningError> {
        self.validate_loaded()?;

        for id in self.player_roads.keys().chain(self.zones.keys()) {
            if scenario.contains_entity(id) {
                return Err(PlanningError::ScenarioEntityIdReserved(id.clone()));
            }
        }
        for id in &self.suppressed_scenario_entities {
            if !scenario.contains_entity(id) {
                return Err(PlanningError::UnknownScenarioEntity(id.clone()));
            }
        }
        Ok(())
    }

    fn validate_loaded(&self) -> Result<(), PlanningError> {
        for (key, road) in &self.player_roads {
            validate_id(key)?;
            validate_id(&road.id)?;
            if key != &road.id {
                return Err(PlanningError::MismatchedPlayerEntityKey {
                    key: key.clone(),
                    id: road.id.clone(),
                });
            }
            if !valid_road_geometry(&road.geometry) {
                return Err(PlanningError::InvalidRoadGeometry);
            }
            if self.zones.contains_key(key) {
                return Err(PlanningError::ConflictingPlayerEntityId(key.clone()));
            }
        }

        for (key, zone) in &self.zones {
            validate_id(key)?;
            validate_id(&zone.id)?;
            if key != &zone.id {
                return Err(PlanningError::MismatchedPlayerEntityKey {
                    key: key.clone(),
                    id: zone.id.clone(),
                });
            }
            if !valid_zone_geometry(&zone.geometry) {
                return Err(PlanningError::InvalidZoneGeometry);
            }
        }

        for id in &self.suppressed_scenario_entities {
            validate_id(id)?;
        }
        Ok(())
    }

    fn add_road(
        &mut self,
        scenario: &CityScenario,
        road: PlannedRoad,
    ) -> Result<PlanningOutcome, PlanningError> {
        validate_id(&road.id)?;
        if !valid_road_geometry(&road.geometry) {
            return Err(PlanningError::InvalidRoadGeometry);
        }
        self.ensure_player_id_available(scenario, &road.id, self.zones.contains_key(&road.id))?;

        if let Some(existing) = self.player_roads.get(&road.id) {
            return if existing == &road {
                Ok(PlanningOutcome::Unchanged)
            } else {
                Err(PlanningError::ConflictingPlayerEntityId(road.id))
            };
        }

        self.player_roads.insert(road.id.clone(), road);
        Ok(PlanningOutcome::Applied)
    }

    fn zone_area(
        &mut self,
        scenario: &CityScenario,
        zone: PlannedZone,
    ) -> Result<PlanningOutcome, PlanningError> {
        validate_id(&zone.id)?;
        if !valid_zone_geometry(&zone.geometry) {
            return Err(PlanningError::InvalidZoneGeometry);
        }
        self.ensure_player_id_available(
            scenario,
            &zone.id,
            self.player_roads.contains_key(&zone.id),
        )?;

        if let Some(existing) = self.zones.get(&zone.id) {
            return if existing == &zone {
                Ok(PlanningOutcome::Unchanged)
            } else {
                Err(PlanningError::ConflictingPlayerEntityId(zone.id))
            };
        }

        self.zones.insert(zone.id.clone(), zone);
        Ok(PlanningOutcome::Applied)
    }

    fn ensure_player_id_available(
        &self,
        scenario: &CityScenario,
        id: &str,
        used_by_other_player_entity: bool,
    ) -> Result<(), PlanningError> {
        if scenario.contains_entity(id) {
            return Err(PlanningError::ScenarioEntityIdReserved(id.to_owned()));
        }
        if used_by_other_player_entity {
            return Err(PlanningError::ConflictingPlayerEntityId(id.to_owned()));
        }
        Ok(())
    }
}

impl CitySave {
    pub(crate) fn apply_planning(
        &mut self,
        command: PlanningCommand,
    ) -> Result<PlanningOutcome, PlanningError> {
        self.world.planning.apply(&self.scenario, command)
    }

    pub(crate) fn effective_roads(&self) -> Vec<EffectiveRoad> {
        self.world.planning.effective_roads(&self.scenario)
    }

    pub(crate) fn restart(&mut self) -> Result<(), PopulationError> {
        let population = self.scenario_population_baseline()?;
        self.world = CityWorld {
            population,
            ..CityWorld::default()
        };
        Ok(())
    }
}

fn validate_id(id: &str) -> Result<(), PlanningError> {
    if id.trim().is_empty() {
        Err(PlanningError::InvalidId)
    } else {
        Ok(())
    }
}

fn valid_road_geometry(geometry: &Geometry) -> bool {
    match geometry {
        Geometry::LineString { coordinates } => coordinates.len() >= 2,
        Geometry::MultiLineString { coordinates } => coordinates.iter().any(|line| line.len() >= 2),
        Geometry::GeometryCollection { geometries } => geometries.iter().any(valid_road_geometry),
        Geometry::Point { .. }
        | Geometry::MultiPoint { .. }
        | Geometry::Polygon { .. }
        | Geometry::MultiPolygon { .. } => false,
    }
}

fn valid_zone_geometry(geometry: &Geometry) -> bool {
    matches!(
        geometry,
        Geometry::Polygon { .. } | Geometry::MultiPolygon { .. }
    )
}

#[cfg(test)]
mod tests {
    use crate::{
        ExternalRevision, SCENARIO_SCHEMA_VERSION, ScenarioBuilding, ScenarioProvenance,
        ScenarioRoad,
    };

    use super::*;

    fn scenario() -> CityScenario {
        CityScenario {
            schema_version: SCENARIO_SCHEMA_VERSION,
            provenance: ScenarioProvenance {
                source_format: "osm-pbf".to_owned(),
                source_name: "fixture.osm.pbf".to_owned(),
                source_sha256: "source-hash".to_owned(),
                parser: ExternalRevision {
                    repository: "geo-analysis".to_owned(),
                    revision: "parser-revision".to_owned(),
                },
            },
            roads: vec![ScenarioRoad {
                id: "imported/way/10".to_owned(),
                source_id: "way/10".to_owned(),
                geometry: line(8.0),
                class: RoadClass::Residential,
                name: Some("Imported Street".to_owned()),
                lanes: Some(2),
                max_speed_kph: Some(50),
            }],
            buildings: vec![ScenarioBuilding {
                id: "imported/way/20".to_owned(),
                source_id: "way/20".to_owned(),
                footprint: polygon(8.001),
                use_kind: crate::BuildingUse::Residential,
                name: None,
                levels: Some(3),
                height_m: Some(9.0),
                gross_floor_area_m2: 900,
            }],
            water: Vec::new(),
            land_use_areas: Vec::new(),
            transit_anchors: Vec::new(),
        }
    }

    fn line(lon: f64) -> Geometry {
        Geometry::LineString {
            coordinates: vec![[lon, 48.0], [lon + 0.001, 48.001]],
        }
    }

    fn polygon(lon: f64) -> Geometry {
        Geometry::Polygon {
            coordinates: vec![vec![
                [lon, 48.0],
                [lon + 0.001, 48.0],
                [lon + 0.001, 48.001],
                [lon, 48.0],
            ]],
        }
    }

    #[test]
    fn repeated_commands_are_idempotent_but_conflicting_reuse_fails_closed() {
        let mut save = CitySave::new(scenario()).unwrap();
        let road = PlannedRoad {
            id: "player/road/1".to_owned(),
            geometry: line(8.01),
            class: RoadClass::Residential,
            name: Some("New Street".to_owned()),
        };
        let command = PlanningCommand::AddRoad { road: road.clone() };

        assert_eq!(
            save.apply_planning(command.clone()).unwrap(),
            PlanningOutcome::Applied
        );
        assert_eq!(
            save.apply_planning(command).unwrap(),
            PlanningOutcome::Unchanged
        );

        let conflicting = PlannedRoad {
            geometry: line(8.02),
            ..road
        };
        assert_eq!(
            save.apply_planning(PlanningCommand::AddRoad { road: conflicting }),
            Err(PlanningError::ConflictingPlayerEntityId(
                "player/road/1".to_owned()
            ))
        );
    }

    #[test]
    fn planning_never_mutates_imported_scenario() {
        let mut save = CitySave::new(scenario()).unwrap();
        let original = save.scenario.clone();

        save.apply_planning(PlanningCommand::SuppressScenarioEntity {
            id: "imported/way/10".to_owned(),
        })
        .unwrap();
        save.apply_planning(PlanningCommand::ZoneArea {
            zone: PlannedZone {
                id: "player/zone/1".to_owned(),
                geometry: polygon(8.03),
                kind: ZoneKind::Residential,
            },
        })
        .unwrap();

        assert_eq!(save.scenario, original);
        assert!(save.world.planning.is_suppressed("imported/way/10"));
    }

    #[test]
    fn effective_roads_merge_scenario_and_player_state_deterministically() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.apply_planning(PlanningCommand::AddRoad {
            road: PlannedRoad {
                id: "player/road/1".to_owned(),
                geometry: line(8.01),
                class: RoadClass::Secondary,
                name: None,
            },
        })
        .unwrap();

        let roads = save.effective_roads();
        assert_eq!(roads.len(), 2);
        assert_eq!(roads[0].id, "imported/way/10");
        assert_eq!(roads[0].origin, EffectiveRoadOrigin::Scenario);
        assert_eq!(roads[1].id, "player/road/1");
        assert_eq!(roads[1].origin, EffectiveRoadOrigin::Player);

        save.apply_planning(PlanningCommand::SuppressScenarioEntity {
            id: "imported/way/10".to_owned(),
        })
        .unwrap();
        let roads = save.effective_roads();
        assert_eq!(roads.len(), 1);
        assert_eq!(roads[0].id, "player/road/1");
    }

    #[test]
    fn save_deserialization_rejects_invalid_planning_state() {
        let mut mismatched_key = CitySave::new(scenario()).unwrap();
        mismatched_key.world.planning.player_roads.insert(
            "slot".to_owned(),
            PlannedRoad {
                id: "player/road/1".to_owned(),
                geometry: line(8.01),
                class: RoadClass::Residential,
                name: None,
            },
        );
        let encoded = serde_json::to_string(&mismatched_key).unwrap();
        let error = serde_json::from_str::<CitySave>(&encoded).unwrap_err();
        assert!(error.to_string().contains("does not match nested id"));

        let mut scenario_collision = CitySave::new(scenario()).unwrap();
        scenario_collision.world.planning.player_roads.insert(
            "imported/way/10".to_owned(),
            PlannedRoad {
                id: "imported/way/10".to_owned(),
                geometry: line(8.01),
                class: RoadClass::Residential,
                name: None,
            },
        );
        let encoded = serde_json::to_string(&scenario_collision).unwrap();
        let error = serde_json::from_str::<CitySave>(&encoded).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("reserved by the imported scenario")
        );

        let mut invalid_geometry = CitySave::new(scenario()).unwrap();
        invalid_geometry.world.planning.player_roads.insert(
            "player/road/1".to_owned(),
            PlannedRoad {
                id: "player/road/1".to_owned(),
                geometry: Geometry::Point {
                    coordinates: [8.01, 48.0],
                },
                class: RoadClass::Residential,
                name: None,
            },
        );
        let encoded = serde_json::to_string(&invalid_geometry).unwrap();
        let error = serde_json::from_str::<CitySave>(&encoded).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("must contain at least one line segment")
        );
    }

    #[test]
    fn save_roundtrip_and_restart_preserve_canonical_scenario_boundary() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.advance_tick().unwrap();
        save.apply_planning(PlanningCommand::ZoneArea {
            zone: PlannedZone {
                id: "player/zone/1".to_owned(),
                geometry: polygon(8.03),
                kind: ZoneKind::MixedUse,
            },
        })
        .unwrap();

        let encoded = serde_json::to_string(&save).unwrap();
        assert!(!encoded.contains("\"tags\""));
        assert!(!encoded.contains("\"highway\""));
        let decoded: CitySave = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, save);

        let scenario = save.scenario.clone();
        let time = save.time;
        let ruleset = save.ruleset.clone();
        let population = save.scenario_population_baseline().unwrap();
        save.restart().unwrap();
        assert_eq!(save.scenario, scenario);
        assert_eq!(save.time, time);
        assert_eq!(save.ruleset, ruleset);
        assert_eq!(save.world.population, population);
        assert_eq!(save.world.planning, CityPlanningOverlay::default());
        assert_eq!(save.world.tick, 0);
    }
}
