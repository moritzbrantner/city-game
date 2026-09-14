use std::fmt;

use crate::{
    CitySave, CityTimeError, CityTimePosition, EffectiveRoad, PopulationCapacity, PopulationError,
    RciDemand, RuleStatus, RuleSystem,
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
    pub fn time_position(&self) -> Result<CityTimePosition, CityTimeError> {
        self.save.time_position()
    }

    #[must_use]
    pub fn effective_roads(&self) -> Vec<EffectiveRoad> {
        self.save.effective_roads()
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
