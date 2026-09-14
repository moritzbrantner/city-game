use std::fmt;

use crate::{
    CitySave, CitySaveError, CityTimePosition, PlanningCommand, PlanningError, PlanningOutcome,
    PopulationError, RuleSystem, RulesetError,
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
    pub fn execute(&mut self, command: CityCommand) -> Result<CityCommandOutcome, CityCommandError> {
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
                self.restart()?;
                Ok(CityCommandOutcome::Restarted)
            }
        }
    }
}
