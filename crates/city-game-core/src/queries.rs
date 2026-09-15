use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    CityPlanningOverlay, CityRuleset, CitySave, CityScenario, CityTimeError, CityTimePosition,
    EffectiveRoad, PopulationCapacity, PopulationError, PopulationState, RciDemand, RuleStatus,
    RuleSystem,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CityQuery {
    Scenario,
    Ruleset,
    Planning,
    Metrics,
    TimePosition,
    EffectiveRoads,
    PopulationState,
    DevelopedPopulationCapacity,
    RciDemand,
    RuleStatus { system: RuleSystem },
    SystemUnlocked { system: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CityQueryResult {
    Scenario { scenario: CityScenario },
    Ruleset { ruleset: CityRuleset },
    Planning { planning: CityPlanningOverlay },
    Metrics { metrics: BTreeMap<String, i64> },
    TimePosition { position: CityTimePosition },
    EffectiveRoads { roads: Vec<EffectiveRoad> },
    PopulationState { state: PopulationState },
    DevelopedPopulationCapacity { capacity: PopulationCapacity },
    RciDemand { demand: RciDemand },
    RuleStatus { status: RuleStatus },
    SystemUnlocked { unlocked: bool },
}

#[derive(Debug, Clone, Copy)]
pub struct CityQueries<'a> {
    save: &'a CitySave,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CityQueryError {
    SystemDisabled(RuleSystem),
    Time(CityTimeError),
    Population(PopulationError),
}

impl fmt::Display for CityQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SystemDisabled(system) => write!(formatter, "{system:?} rules are disabled"),
            Self::Time(error) => error.fmt(formatter),
            Self::Population(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for CityQueryError {}

impl From<CityTimeError> for CityQueryError {
    fn from(error: CityTimeError) -> Self {
        Self::Time(error)
    }
}

impl From<PopulationError> for CityQueryError {
    fn from(error: PopulationError) -> Self {
        Self::Population(error)
    }
}

impl CitySave {
    #[must_use]
    pub fn queries(&self) -> CityQueries<'_> {
        CityQueries { save: self }
    }

    pub fn query(&self, query: CityQuery) -> Result<CityQueryResult, CityQueryError> {
        let queries = self.queries();
        match query {
            CityQuery::Scenario => Ok(CityQueryResult::Scenario {
                scenario: queries.scenario().clone(),
            }),
            CityQuery::Ruleset => Ok(CityQueryResult::Ruleset {
                ruleset: queries.ruleset().clone(),
            }),
            CityQuery::Planning => Ok(CityQueryResult::Planning {
                planning: queries.planning()?.clone(),
            }),
            CityQuery::Metrics => Ok(CityQueryResult::Metrics {
                metrics: queries.metrics().clone(),
            }),
            CityQuery::TimePosition => Ok(CityQueryResult::TimePosition {
                position: queries.time_position()?,
            }),
            CityQuery::EffectiveRoads => Ok(CityQueryResult::EffectiveRoads {
                roads: queries.effective_roads()?,
            }),
            CityQuery::PopulationState => Ok(CityQueryResult::PopulationState {
                state: queries.population_state()?,
            }),
            CityQuery::DevelopedPopulationCapacity => {
                Ok(CityQueryResult::DevelopedPopulationCapacity {
                    capacity: queries.developed_population_capacity()?,
                })
            }
            CityQuery::RciDemand => Ok(CityQueryResult::RciDemand {
                demand: queries.rci_demand()?,
            }),
            CityQuery::RuleStatus { system } => Ok(CityQueryResult::RuleStatus {
                status: queries.rule_status(system),
            }),
            CityQuery::SystemUnlocked { system } => Ok(CityQueryResult::SystemUnlocked {
                unlocked: queries.system_unlocked(&system)?,
            }),
        }
    }
}

impl CityQueries<'_> {
    #[must_use]
    pub fn scenario(&self) -> &CityScenario {
        &self.save.scenario
    }

    #[must_use]
    pub fn ruleset(&self) -> &CityRuleset {
        &self.save.ruleset
    }

    pub fn planning(&self) -> Result<&CityPlanningOverlay, CityQueryError> {
        self.require_enabled(RuleSystem::Planning)?;
        Ok(&self.save.world.planning)
    }

    #[must_use]
    pub fn metrics(&self) -> &BTreeMap<String, i64> {
        &self.save.world.metrics
    }

    pub fn time_position(&self) -> Result<CityTimePosition, CityTimeError> {
        self.save.time_position()
    }

    pub fn effective_roads(&self) -> Result<Vec<EffectiveRoad>, CityQueryError> {
        self.require_enabled(RuleSystem::Planning)?;
        Ok(self.save.effective_roads())
    }

    pub fn population_state(&self) -> Result<PopulationState, CityQueryError> {
        self.require_enabled(RuleSystem::Population)?;
        Ok(self.save.world.population)
    }

    pub fn developed_population_capacity(&self) -> Result<PopulationCapacity, CityQueryError> {
        self.require_enabled(RuleSystem::Population)?;
        Ok(self.save.developed_population_capacity()?)
    }

    pub fn rci_demand(&self) -> Result<RciDemand, CityQueryError> {
        self.require_enabled(RuleSystem::Population)?;
        Ok(self.save.rci_demand()?)
    }

    #[must_use]
    pub fn rule_status(&self, system: RuleSystem) -> RuleStatus {
        self.save.ruleset.status(system)
    }

    pub fn system_unlocked(&self, system: &str) -> Result<bool, CityQueryError> {
        self.require_enabled(RuleSystem::Progression)?;
        Ok(self.save.world.progression.unlocked.contains(system))
    }

    fn require_enabled(&self, system: RuleSystem) -> Result<(), CityQueryError> {
        if self.save.ruleset.is_enabled(system) {
            Ok(())
        } else {
            Err(CityQueryError::SystemDisabled(system))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        CityScenario, ExternalRevision, RuleStatus, SCENARIO_SCHEMA_VERSION, ScenarioProvenance,
    };

    use super::*;

    fn scenario() -> CityScenario {
        CityScenario {
            schema_version: SCENARIO_SCHEMA_VERSION,
            provenance: ScenarioProvenance {
                source_format: "fixture".to_owned(),
                source_name: "queries".to_owned(),
                source_sha256: "queries".to_owned(),
                parser: ExternalRevision {
                    repository: "fixture".to_owned(),
                    revision: "fixture".to_owned(),
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
    fn queries_roundtrip_as_stable_tagged_application_contract() {
        let query = CityQuery::RuleStatus {
            system: RuleSystem::Population,
        };
        let encoded = serde_json::to_value(&query).unwrap();
        let decoded: CityQuery = serde_json::from_value(encoded.clone()).unwrap();

        assert_eq!(decoded, query);
        assert_eq!(encoded["kind"], "ruleStatus");
        assert_eq!(encoded["system"], "population");
    }

    #[test]
    fn query_dispatch_returns_transport_safe_result() {
        let save = CitySave::new(scenario()).unwrap();
        let result = save.query(CityQuery::TimePosition).unwrap();
        let encoded = serde_json::to_value(result).unwrap();

        assert_eq!(encoded["kind"], "timePosition");
        assert_eq!(encoded["position"]["tick"], 0);
    }

    #[test]
    fn disabled_population_is_explicit_on_read_side() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.ruleset
            .set_status(RuleSystem::Population, RuleStatus::Disabled);

        assert_eq!(
            save.queries().rci_demand(),
            Err(CityQueryError::SystemDisabled(RuleSystem::Population))
        );
        assert_eq!(
            save.queries().population_state(),
            Err(CityQueryError::SystemDisabled(RuleSystem::Population))
        );
        assert_eq!(
            save.query(CityQuery::PopulationState),
            Err(CityQueryError::SystemDisabled(RuleSystem::Population))
        );
        assert_eq!(
            save.queries().rule_status(RuleSystem::Population),
            RuleStatus::Disabled
        );
    }

    #[test]
    fn disabled_planning_and_progression_are_explicit_on_read_side() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.ruleset
            .set_status(RuleSystem::Planning, RuleStatus::Disabled);
        save.ruleset
            .set_status(RuleSystem::Progression, RuleStatus::Disabled);

        assert_eq!(
            save.queries().planning(),
            Err(CityQueryError::SystemDisabled(RuleSystem::Planning))
        );
        assert_eq!(
            save.query(CityQuery::EffectiveRoads),
            Err(CityQueryError::SystemDisabled(RuleSystem::Planning))
        );
        assert_eq!(
            save.query(CityQuery::SystemUnlocked {
                system: "basic-services".to_owned(),
            }),
            Err(CityQueryError::SystemDisabled(RuleSystem::Progression))
        );
    }

    #[test]
    fn immutable_configuration_and_unrelated_reads_remain_available() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.ruleset
            .set_status(RuleSystem::Population, RuleStatus::Disabled);

        assert_eq!(save.queries().time_position().unwrap().tick, 0);
        assert!(save.queries().effective_roads().unwrap().is_empty());
        assert_eq!(save.queries().scenario().provenance.source_name, "queries");
        assert_eq!(
            save.queries().ruleset().population.status,
            RuleStatus::Disabled
        );
        assert!(save.queries().planning().unwrap().zones.is_empty());
        assert!(save.queries().metrics().is_empty());
    }
}
