use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{BuildingUse, CitySave, CityScenario, ScenarioBuilding};

const BASIS_POINTS: u64 = 10_000;
const MAX_DEMAND_PRESSURE: i32 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PopulationRules {
    pub residential_floor_area_m2_per_household: u32,
    pub commercial_floor_area_m2_per_job: u32,
    pub industrial_floor_area_m2_per_job: u32,
    pub initial_occupancy_basis_points: u16,
    pub target_occupancy_basis_points: u16,
}

impl PopulationRules {
    pub fn validate(self) -> Result<(), PopulationError> {
        if self.residential_floor_area_m2_per_household == 0 {
            return Err(PopulationError::InvalidRule(
                "residential floor area per household must be positive",
            ));
        }
        if self.commercial_floor_area_m2_per_job == 0 {
            return Err(PopulationError::InvalidRule(
                "commercial floor area per job must be positive",
            ));
        }
        if self.industrial_floor_area_m2_per_job == 0 {
            return Err(PopulationError::InvalidRule(
                "industrial floor area per job must be positive",
            ));
        }
        if u64::from(self.initial_occupancy_basis_points) > BASIS_POINTS {
            return Err(PopulationError::InvalidRule(
                "initial occupancy must be between 0 and 10000 basis points",
            ));
        }
        if u64::from(self.target_occupancy_basis_points) > BASIS_POINTS {
            return Err(PopulationError::InvalidRule(
                "target occupancy must be between 0 and 10000 basis points",
            ));
        }
        Ok(())
    }
}

impl Default for PopulationRules {
    fn default() -> Self {
        Self {
            residential_floor_area_m2_per_household: 90,
            commercial_floor_area_m2_per_job: 35,
            industrial_floor_area_m2_per_job: 60,
            initial_occupancy_basis_points: 9_000,
            target_occupancy_basis_points: 9_000,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PopulationCapacity {
    pub households: u64,
    pub commercial_jobs: u64,
    pub industrial_jobs: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PopulationState {
    pub households: u64,
    pub commercial_jobs: u64,
    pub industrial_jobs: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RciDemand {
    pub residential: i32,
    pub commercial: i32,
    pub industrial: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopulationError {
    InvalidRule(&'static str),
    CapacityOverflow,
}

impl fmt::Display for PopulationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRule(message) => formatter.write_str(message),
            Self::CapacityOverflow => formatter.write_str("aggregate population capacity overflow"),
        }
    }
}

impl std::error::Error for PopulationError {}

impl PopulationCapacity {
    pub fn from_scenario(
        scenario: &CityScenario,
        rules: PopulationRules,
    ) -> Result<Self, PopulationError> {
        capacity_from_buildings(scenario.buildings.iter(), rules)
    }
}

impl PopulationState {
    pub fn baseline_from_scenario(
        scenario: &CityScenario,
        rules: PopulationRules,
    ) -> Result<Self, PopulationError> {
        let capacity = PopulationCapacity::from_scenario(scenario, rules)?;
        Ok(Self {
            households: apply_basis_points(
                capacity.households,
                rules.initial_occupancy_basis_points,
            ),
            commercial_jobs: apply_basis_points(
                capacity.commercial_jobs,
                rules.initial_occupancy_basis_points,
            ),
            industrial_jobs: apply_basis_points(
                capacity.industrial_jobs,
                rules.initial_occupancy_basis_points,
            ),
        })
    }

    pub fn demand(
        self,
        capacity: PopulationCapacity,
        rules: PopulationRules,
    ) -> Result<RciDemand, PopulationError> {
        rules.validate()?;
        Ok(RciDemand {
            residential: demand_pressure(
                self.households,
                capacity.households,
                rules.target_occupancy_basis_points,
            ),
            commercial: demand_pressure(
                self.commercial_jobs,
                capacity.commercial_jobs,
                rules.target_occupancy_basis_points,
            ),
            industrial: demand_pressure(
                self.industrial_jobs,
                capacity.industrial_jobs,
                rules.target_occupancy_basis_points,
            ),
        })
    }
}

impl CitySave {
    pub fn validate_population_configuration(&self) -> Result<(), PopulationError> {
        PopulationCapacity::from_scenario(&self.scenario, self.population_rules).map(|_| ())
    }

    pub fn developed_population_capacity(&self) -> Result<PopulationCapacity, PopulationError> {
        capacity_from_buildings(
            self.scenario
                .buildings
                .iter()
                .filter(|building| !self.world.planning.is_suppressed(&building.id)),
            self.population_rules,
        )
    }

    pub fn rci_demand(&self) -> Result<RciDemand, PopulationError> {
        self.world
            .population
            .demand(self.developed_population_capacity()?, self.population_rules)
    }

    pub(crate) fn scenario_population_baseline(&self) -> Result<PopulationState, PopulationError> {
        PopulationState::baseline_from_scenario(&self.scenario, self.population_rules)
    }
}

fn capacity_from_buildings<'a>(
    buildings: impl Iterator<Item = &'a ScenarioBuilding>,
    rules: PopulationRules,
) -> Result<PopulationCapacity, PopulationError> {
    rules.validate()?;
    let mut capacity = PopulationCapacity::default();

    for building in buildings {
        let (target, divisor) = match building.use_kind {
            BuildingUse::Residential => (
                &mut capacity.households,
                rules.residential_floor_area_m2_per_household,
            ),
            BuildingUse::Commercial => (
                &mut capacity.commercial_jobs,
                rules.commercial_floor_area_m2_per_job,
            ),
            BuildingUse::Industrial => (
                &mut capacity.industrial_jobs,
                rules.industrial_floor_area_m2_per_job,
            ),
            BuildingUse::Civic | BuildingUse::Agricultural | BuildingUse::Other => continue,
        };

        let building_capacity = building.gross_floor_area_m2 / u64::from(divisor);
        *target = target
            .checked_add(building_capacity)
            .ok_or(PopulationError::CapacityOverflow)?;
    }

    Ok(capacity)
}

fn apply_basis_points(value: u64, basis_points: u16) -> u64 {
    let scaled = u128::from(value) * u128::from(basis_points);
    u64::try_from(scaled / u128::from(BASIS_POINTS))
        .expect("occupancy at or below 10000 basis points cannot exceed input capacity")
}

fn demand_pressure(occupied: u64, capacity: u64, target_basis_points: u16) -> i32 {
    if capacity == 0 {
        return if occupied == 0 {
            0
        } else {
            MAX_DEMAND_PRESSURE
        };
    }

    let target = apply_basis_points(capacity, target_basis_points).max(1);
    let occupied = i128::from(occupied);
    let target = i128::from(target);
    let pressure = (occupied - target) * i128::from(BASIS_POINTS) / target;
    pressure
        .clamp(
            i128::from(-MAX_DEMAND_PRESSURE),
            i128::from(MAX_DEMAND_PRESSURE),
        )
        .try_into()
        .expect("clamped demand fits i32")
}

#[cfg(test)]
mod tests {
    use geo_core::Geometry;

    use crate::{
        CityPlanningOverlay, CityWorld, ExternalRevision, PlanningCommand, SCENARIO_SCHEMA_VERSION,
        ScenarioProvenance,
    };

    use super::*;

    fn building(id: &str, use_kind: BuildingUse, gross_floor_area_m2: u64) -> ScenarioBuilding {
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
            use_kind,
            name: None,
            levels: Some(1),
            height_m: Some(3.0),
            gross_floor_area_m2,
        }
    }

    fn scenario() -> CityScenario {
        CityScenario {
            schema_version: SCENARIO_SCHEMA_VERSION,
            provenance: ScenarioProvenance {
                source_format: "fixture".to_owned(),
                source_name: "fixture".to_owned(),
                source_sha256: "fixture".to_owned(),
                parser: ExternalRevision {
                    repository: "fixture".to_owned(),
                    revision: "fixture".to_owned(),
                },
            },
            roads: Vec::new(),
            buildings: vec![
                building("residential", BuildingUse::Residential, 9_000),
                building("commercial", BuildingUse::Commercial, 3_500),
                building("industrial", BuildingUse::Industrial, 6_000),
            ],
            water: Vec::new(),
            land_use_areas: Vec::new(),
            transit_anchors: Vec::new(),
        }
    }

    #[test]
    fn canonical_building_stock_derives_stable_aggregate_baseline() {
        let rules = PopulationRules::default();
        let capacity = PopulationCapacity::from_scenario(&scenario(), rules).unwrap();
        let baseline = PopulationState::baseline_from_scenario(&scenario(), rules).unwrap();

        assert_eq!(capacity.households, 100);
        assert_eq!(capacity.commercial_jobs, 100);
        assert_eq!(capacity.industrial_jobs, 100);
        assert_eq!(baseline.households, 90);
        assert_eq!(baseline.commercial_jobs, 90);
        assert_eq!(baseline.industrial_jobs, 90);
        assert_eq!(
            baseline.demand(capacity, rules).unwrap(),
            RciDemand::default()
        );
    }

    #[test]
    fn redevelopment_reduces_capacity_without_deleting_occupants() {
        let rules = PopulationRules::default();
        let population = PopulationState::baseline_from_scenario(&scenario(), rules).unwrap();
        let mut save = CitySave {
            schema_version: crate::SAVE_SCHEMA_VERSION,
            scenario: scenario(),
            time: crate::CityTimeConfig::default(),
            population_rules: rules,
            world: CityWorld {
                population,
                planning: CityPlanningOverlay::default(),
                ..CityWorld::default()
            },
        };

        save.apply_planning(PlanningCommand::SuppressScenarioEntity {
            id: "residential".to_owned(),
        })
        .unwrap();

        assert_eq!(save.world.population, population);
        assert_eq!(save.developed_population_capacity().unwrap().households, 0);
        assert_eq!(save.rci_demand().unwrap().residential, MAX_DEMAND_PRESSURE);
    }

    #[test]
    fn invalid_rules_fail_before_capacity_is_derived() {
        let rules = PopulationRules {
            residential_floor_area_m2_per_household: 0,
            ..PopulationRules::default()
        };

        assert!(matches!(
            PopulationCapacity::from_scenario(&scenario(), rules),
            Err(PopulationError::InvalidRule(_))
        ));
    }

    #[test]
    fn save_construction_returns_capacity_overflow() {
        let mut overflowing = scenario();
        overflowing.buildings = vec![
            building("residential/1", BuildingUse::Residential, u64::MAX),
            building("residential/2", BuildingUse::Residential, u64::MAX),
        ];
        let rules = PopulationRules {
            residential_floor_area_m2_per_household: 1,
            ..PopulationRules::default()
        };
        let population = PopulationState::baseline_from_scenario(&overflowing, rules);
        assert_eq!(population, Err(PopulationError::CapacityOverflow));

        // `CitySave::new` uses the default divisor of 90; enough max-sized
        // residential buildings still exceed u64 aggregate capacity.
        overflowing.buildings = (0..100)
            .map(|index| {
                building(
                    &format!("residential/{index}"),
                    BuildingUse::Residential,
                    u64::MAX,
                )
            })
            .collect();
        assert_eq!(
            CitySave::new(overflowing),
            Err(PopulationError::CapacityOverflow)
        );
    }
}
