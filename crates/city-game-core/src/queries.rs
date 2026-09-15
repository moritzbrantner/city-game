use std::{collections::BTreeMap, fmt};

use crate::{
    CityPlanningOverlay, CityRuleset, CitySave, CityScenario, CityTimeError, CityTimePosition,
    EffectiveRoad, PopulationCapacity, PopulationError, PopulationState, RciDemand, RuleStatus,
    RuleSystem,
};

#[derive(Debug, Clone, Copy)]
pub struct CityQueries<'a> {
    save: &'a CitySave,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CityQueryError {
    SystemDisabled(RuleSystem),
    Population(PopulationError),
}

impl fmt::Display for CityQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SystemDisabled(system) => write!(formatter, "{system:?} rules are disabled"),
            Self::Population(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for CityQueryError {}

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

    #[must_use]
    pub fn planning(&self) -> &CityPlanningOverlay {
        &self.save.world.planning
    }

    #[must_use]
    pub fn metrics(&self) -> &BTreeMap<String, i64> {
        &self.save.world.metrics
    }

    pub fn time_position(&self) -> Result<CityTimePosition, CityTimeError> {
        self.save.time_position()
    }

    #[must_use]
    pub fn effective_roads(&self) -> Vec<EffectiveRoad> {
        self.save.effective_roads()
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

    #[must_use]
    pub fn system_unlocked(&self, system: &str) -> bool {
        self.save.world.progression.unlocked.contains(system)
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
            save.queries().rule_status(RuleSystem::Population),
            RuleStatus::Disabled
        );
    }

    #[test]
    fn immutable_configuration_and_unrelated_reads_remain_available() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.ruleset
            .set_status(RuleSystem::Population, RuleStatus::Disabled);

        assert_eq!(save.queries().time_position().unwrap().tick, 0);
        assert!(save.queries().effective_roads().is_empty());
        assert_eq!(save.queries().scenario().provenance.source_name, "queries");
        assert_eq!(
            save.queries().ruleset().population.status,
            RuleStatus::Disabled
        );
        assert!(save.queries().planning().zones.is_empty());
        assert!(save.queries().metrics().is_empty());
    }
}
