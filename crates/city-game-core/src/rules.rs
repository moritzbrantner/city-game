use std::{collections::BTreeSet, fmt};

use serde::{Deserialize, Serialize};

use crate::{PopulationError, PopulationRules, ProgressionRule};

pub const RULESET_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleSystem {
    Planning,
    Population,
    Progression,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleStatus {
    Disabled,
    #[default]
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleModule<T> {
    pub status: RuleStatus,
    pub config: T,
}

impl<T: Default> Default for RuleModule<T> {
    fn default() -> Self {
        Self {
            status: RuleStatus::Enabled,
            config: T::default(),
        }
    }
}

impl<T> RuleModule<T> {
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.status == RuleStatus::Enabled
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningRules {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressionRules {
    pub rules: Vec<ProgressionRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CityRuleset {
    pub schema_version: u32,
    pub planning: RuleModule<PlanningRules>,
    pub population: RuleModule<PopulationRules>,
    pub progression: RuleModule<ProgressionRules>,
}

impl Default for CityRuleset {
    fn default() -> Self {
        Self {
            schema_version: RULESET_SCHEMA_VERSION,
            planning: RuleModule::default(),
            population: RuleModule::default(),
            progression: RuleModule::default(),
        }
    }
}

impl CityRuleset {
    #[must_use]
    pub fn is_enabled(&self, system: RuleSystem) -> bool {
        self.status(system) == RuleStatus::Enabled
    }

    #[must_use]
    pub fn status(&self, system: RuleSystem) -> RuleStatus {
        match system {
            RuleSystem::Planning => self.planning.status,
            RuleSystem::Population => self.population.status,
            RuleSystem::Progression => self.progression.status,
        }
    }

    pub fn set_status(&mut self, system: RuleSystem, status: RuleStatus) {
        match system {
            RuleSystem::Planning => self.planning.status = status,
            RuleSystem::Population => self.population.status = status,
            RuleSystem::Progression => self.progression.status = status,
        }
    }

    pub fn validate(&self) -> Result<(), RulesetError> {
        if self.schema_version != RULESET_SCHEMA_VERSION {
            return Err(RulesetError::UnsupportedSchemaVersion(self.schema_version));
        }

        self.population
            .config
            .validate()
            .map_err(RulesetError::Population)?;

        let mut ids = BTreeSet::new();
        for rule in &self.progression.config.rules {
            if rule.id.trim().is_empty() || rule.unlocks.trim().is_empty() {
                return Err(RulesetError::InvalidProgressionRule);
            }
            if !ids.insert(rule.id.clone()) {
                return Err(RulesetError::DuplicateProgressionRuleId(rule.id.clone()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RulesetError {
    UnsupportedSchemaVersion(u32),
    Population(PopulationError),
    InvalidProgressionRule,
    DuplicateProgressionRuleId(String),
}

impl fmt::Display for RulesetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion(version) => {
                write!(
                    formatter,
                    "unsupported city ruleset schema version {version}"
                )
            }
            Self::Population(error) => error.fmt(formatter),
            Self::InvalidProgressionRule => {
                formatter.write_str("progression rule id and unlock target must be non-empty")
            }
            Self::DuplicateProgressionRuleId(id) => {
                write!(formatter, "duplicate progression rule id {id}")
            }
        }
    }
}

impl std::error::Error for RulesetError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_a_system_preserves_its_typed_configuration() {
        let mut ruleset = CityRuleset::default();
        ruleset
            .population
            .config
            .residential_floor_area_m2_per_household = 72;

        ruleset.set_status(RuleSystem::Population, RuleStatus::Disabled);
        assert!(!ruleset.is_enabled(RuleSystem::Population));
        assert_eq!(
            ruleset
                .population
                .config
                .residential_floor_area_m2_per_household,
            72
        );

        ruleset.set_status(RuleSystem::Population, RuleStatus::Enabled);
        assert!(ruleset.is_enabled(RuleSystem::Population));
        assert_eq!(
            ruleset
                .population
                .config
                .residential_floor_area_m2_per_household,
            72
        );
        assert_eq!(ruleset.validate(), Ok(()));
    }

    #[test]
    fn duplicate_progression_rule_ids_fail_closed() {
        let rule = ProgressionRule {
            id: "waste".to_owned(),
            unlocks: "waste-management".to_owned(),
            all: Vec::new(),
        };
        let mut ruleset = CityRuleset::default();
        ruleset.progression.config.rules = vec![rule.clone(), rule];

        assert_eq!(
            ruleset.validate(),
            Err(RulesetError::DuplicateProgressionRuleId("waste".to_owned()))
        );
    }

    #[test]
    fn invalid_typed_configuration_fails_even_while_disabled() {
        let mut ruleset = CityRuleset::default();
        ruleset.population.status = RuleStatus::Disabled;
        ruleset
            .population
            .config
            .residential_floor_area_m2_per_household = 0;

        assert!(matches!(
            ruleset.validate(),
            Err(RulesetError::Population(PopulationError::InvalidRule(_)))
        ));
    }
}
