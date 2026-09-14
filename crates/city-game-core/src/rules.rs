use std::{collections::BTreeSet, fmt};

use serde::{Deserialize, Serialize};

use crate::ProgressionRule;

pub const RULESET_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleSystem {
    Planning,
    Population,
    Progression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleStatus {
    Disabled,
    Enabled,
}

impl Default for RuleStatus {
    fn default() -> Self {
        Self::Enabled
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CityRuleset {
    pub schema_version: u32,
    #[serde(default)]
    pub planning: RuleStatus,
    #[serde(default)]
    pub population: RuleStatus,
    #[serde(default)]
    pub progression: RuleStatus,
    #[serde(default)]
    pub progression_rules: Vec<ProgressionRule>,
}

impl Default for CityRuleset {
    fn default() -> Self {
        Self {
            schema_version: RULESET_SCHEMA_VERSION,
            planning: RuleStatus::Enabled,
            population: RuleStatus::Enabled,
            progression: RuleStatus::Enabled,
            progression_rules: Vec::new(),
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
            RuleSystem::Planning => self.planning,
            RuleSystem::Population => self.population,
            RuleSystem::Progression => self.progression,
        }
    }

    pub fn set_status(&mut self, system: RuleSystem, status: RuleStatus) {
        match system {
            RuleSystem::Planning => self.planning = status,
            RuleSystem::Population => self.population = status,
            RuleSystem::Progression => self.progression = status,
        }
    }

    pub fn validate(&self) -> Result<(), RulesetError> {
        if self.schema_version != RULESET_SCHEMA_VERSION {
            return Err(RulesetError::UnsupportedSchemaVersion(self.schema_version));
        }

        let mut ids = BTreeSet::new();
        for rule in &self.progression_rules {
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
    InvalidProgressionRule,
    DuplicateProgressionRuleId(String),
}

impl fmt::Display for RulesetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion(version) => {
                write!(formatter, "unsupported city ruleset schema version {version}")
            }
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
    fn rule_systems_can_be_toggled_without_rewriting_other_configuration() {
        let mut ruleset = CityRuleset::default();
        ruleset.set_status(RuleSystem::Population, RuleStatus::Disabled);

        assert!(ruleset.is_enabled(RuleSystem::Planning));
        assert!(!ruleset.is_enabled(RuleSystem::Population));
        assert!(ruleset.is_enabled(RuleSystem::Progression));
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
        ruleset.progression_rules = vec![rule.clone(), rule];

        assert_eq!(
            ruleset.validate(),
            Err(RulesetError::DuplicateProgressionRuleId("waste".to_owned()))
        );
    }
}
