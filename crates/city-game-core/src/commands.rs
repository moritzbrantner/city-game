use std::fmt;

use crate::{
    CitySave, CitySaveError, CityTimePosition, CityWorld, PlanningCommand, PlanningError,
    PlanningOutcome, PopulationError, RuleSystem, RulesetError,
};

#[derive(Debug, Clone, PartialEq)]
pub enum CityCommand {
    Planning(PlanningCommand),
    AdvanceFixedSteps { steps: u64 },
    EvaluateProgression,
    Restart,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CityCommandOutcome {
    Planning(PlanningOutcome),
    Advanced(CityTimePosition),
    ProgressionEvaluated { newly_unlocked: Vec<String> },
    Restarted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CityCommandError {
    Ruleset(RulesetError),
    SystemDisabled(RuleSystem),
    Planning(PlanningError),
    Save(CitySaveError),
    Population(PopulationError),
}

impl fmt::Display for CityCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ruleset(error) => error.fmt(formatter),
            Self::SystemDisabled(system) => write!(formatter, "{system:?} rules are disabled"),
            Self::Planning(error) => error.fmt(formatter),
            Self::Save(error) => error.fmt(formatter),
            Self::Population(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for CityCommandError {}

impl From<RulesetError> for CityCommandError {
    fn from(error: RulesetError) -> Self {
        Self::Ruleset(error)
    }
}

impl From<PlanningError> for CityCommandError {
    fn from(error: PlanningError) -> Self {
        Self::Planning(error)
    }
}

impl From<CitySaveError> for CityCommandError {
    fn from(error: CitySaveError) -> Self {
        Self::Save(error)
    }
}

impl From<PopulationError> for CityCommandError {
    fn from(error: PopulationError) -> Self {
        Self::Population(error)
    }
}

impl CitySave {
    pub fn execute(
        &mut self,
        command: CityCommand,
    ) -> Result<CityCommandOutcome, CityCommandError> {
        self.ruleset.validate()?;

        match command {
            CityCommand::Planning(command) => {
                if !self.ruleset.is_enabled(RuleSystem::Planning) {
                    return Err(CityCommandError::SystemDisabled(RuleSystem::Planning));
                }
                Ok(CityCommandOutcome::Planning(self.apply_planning(command)?))
            }
            CityCommand::AdvanceFixedSteps { steps } => Ok(CityCommandOutcome::Advanced(
                self.advance_fixed_steps(steps)?,
            )),
            CityCommand::EvaluateProgression => {
                if !self.ruleset.is_enabled(RuleSystem::Progression) {
                    return Err(CityCommandError::SystemDisabled(RuleSystem::Progression));
                }
                let rules = self.ruleset.progression_rules.clone();
                let newly_unlocked = self.world.progression.evaluate(&rules, &self.world.metrics);
                Ok(CityCommandOutcome::ProgressionEvaluated { newly_unlocked })
            }
            CityCommand::Restart => {
                if self.ruleset.is_enabled(RuleSystem::Population) {
                    self.restart()?;
                } else {
                    self.world = CityWorld::default();
                }
                Ok(CityCommandOutcome::Restarted)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        CityScenario, ExternalRevision, PopulationRules, ProgressionRule, Requirement, RuleStatus,
        SCENARIO_SCHEMA_VERSION, ScenarioProvenance,
    };

    use super::*;

    fn scenario() -> CityScenario {
        CityScenario {
            schema_version: SCENARIO_SCHEMA_VERSION,
            provenance: ScenarioProvenance {
                source_format: "fixture".to_owned(),
                source_name: "cqrs".to_owned(),
                source_sha256: "cqrs".to_owned(),
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
    fn disabled_planning_rejects_write_without_mutation() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.ruleset
            .set_status(RuleSystem::Planning, RuleStatus::Disabled);
        let before = save.clone();

        assert_eq!(
            save.execute(CityCommand::Planning(PlanningCommand::RemovePlayerRoad {
                id: "player/road/1".to_owned(),
            })),
            Err(CityCommandError::SystemDisabled(RuleSystem::Planning))
        );
        assert_eq!(save, before);
    }

    #[test]
    fn progression_rules_are_configuration_and_evaluate_through_command_gateway() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.world.metrics.insert("population".to_owned(), 1_000);
        save.ruleset.progression_rules = vec![ProgressionRule {
            id: "services".to_owned(),
            unlocks: "basic-services".to_owned(),
            all: vec![Requirement::MetricAtLeast {
                metric: "population".to_owned(),
                value: 1_000,
            }],
        }];

        assert_eq!(
            save.execute(CityCommand::EvaluateProgression).unwrap(),
            CityCommandOutcome::ProgressionEvaluated {
                newly_unlocked: vec!["basic-services".to_owned()]
            }
        );
        assert!(save.world.progression.unlocked.contains("basic-services"));
    }

    #[test]
    fn disabled_progression_rejects_evaluation_without_mutation() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.ruleset
            .set_status(RuleSystem::Progression, RuleStatus::Disabled);
        let before = save.clone();

        assert_eq!(
            save.execute(CityCommand::EvaluateProgression),
            Err(CityCommandError::SystemDisabled(RuleSystem::Progression))
        );
        assert_eq!(save, before);
    }

    #[test]
    fn restart_does_not_require_population_rules_when_population_is_disabled() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.ruleset
            .set_status(RuleSystem::Population, RuleStatus::Disabled);
        save.population_rules = PopulationRules {
            residential_floor_area_m2_per_household: 0,
            ..PopulationRules::default()
        };
        save.world.tick = 7;

        assert_eq!(
            save.execute(CityCommand::Restart).unwrap(),
            CityCommandOutcome::Restarted
        );
        assert_eq!(save.world, CityWorld::default());
    }
}
