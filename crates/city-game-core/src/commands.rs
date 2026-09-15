use std::fmt;

use serde::{Deserialize, Serialize, Serializer};

use crate::{
    CitySave, CityWorld, PlanningCommand, PlanningError, PlanningOutcome, PopulationError,
    RuleSystem, RulesetError,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CityCommand {
    Planning { command: PlanningCommand },
    Restart,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CityCommandOutcome {
    Planning(PlanningOutcome),
    Restarted,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum CityCommandOutcomeWire {
    Planning { outcome: &'static str },
    Restarted,
}

impl Serialize for CityCommandOutcome {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let wire = match self {
            Self::Planning(PlanningOutcome::Applied) => {
                CityCommandOutcomeWire::Planning { outcome: "applied" }
            }
            Self::Planning(PlanningOutcome::Unchanged) => CityCommandOutcomeWire::Planning {
                outcome: "unchanged",
            },
            Self::Restarted => CityCommandOutcomeWire::Restarted,
        };
        wire.serialize(serializer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CityCommandError {
    Ruleset(RulesetError),
    SystemDisabled(RuleSystem),
    Planning(PlanningError),
    Population(PopulationError),
}

impl fmt::Display for CityCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ruleset(error) => error.fmt(formatter),
            Self::SystemDisabled(system) => write!(formatter, "{system:?} rules are disabled"),
            Self::Planning(error) => error.fmt(formatter),
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

impl From<PopulationError> for CityCommandError {
    fn from(error: PopulationError) -> Self {
        Self::Population(error)
    }
}

impl CitySave {
    /// Executes application/player intent.
    ///
    /// Fixed-step simulation and internal system evaluation deliberately do not flow through this
    /// gateway; they use direct deterministic simulation operations instead.
    pub fn execute(
        &mut self,
        command: CityCommand,
    ) -> Result<CityCommandOutcome, CityCommandError> {
        self.ruleset.validate()?;

        match command {
            CityCommand::Planning { command } => {
                if !self.ruleset.is_enabled(RuleSystem::Planning) {
                    return Err(CityCommandError::SystemDisabled(RuleSystem::Planning));
                }
                Ok(CityCommandOutcome::Planning(self.apply_planning(command)?))
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
        CityScenario, ExternalRevision, RuleStatus, SCENARIO_SCHEMA_VERSION, ScenarioProvenance,
    };

    use super::*;

    fn scenario() -> CityScenario {
        CityScenario {
            schema_version: SCENARIO_SCHEMA_VERSION,
            provenance: ScenarioProvenance {
                source_format: "fixture".to_owned(),
                source_name: "commands".to_owned(),
                source_sha256: "commands".to_owned(),
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
    fn commands_roundtrip_as_stable_tagged_application_contract() {
        let command = CityCommand::Planning {
            command: PlanningCommand::RemovePlayerRoad {
                id: "player/road/1".to_owned(),
            },
        };
        let encoded = serde_json::to_value(&command).unwrap();
        let decoded: CityCommand = serde_json::from_value(encoded.clone()).unwrap();

        assert_eq!(decoded, command);
        assert_eq!(encoded["kind"], "planning");
        assert_eq!(encoded["command"]["kind"], "removePlayerRoad");
        assert_eq!(encoded["command"]["id"], "player/road/1");
    }

    #[test]
    fn command_outcomes_have_transport_safe_shape() {
        let planning =
            serde_json::to_value(CityCommandOutcome::Planning(PlanningOutcome::Applied)).unwrap();
        let restarted = serde_json::to_value(CityCommandOutcome::Restarted).unwrap();

        assert_eq!(planning["kind"], "planning");
        assert_eq!(planning["outcome"], "applied");
        assert_eq!(restarted["kind"], "restarted");
    }

    #[test]
    fn simulation_operations_are_not_application_commands() {
        assert!(
            serde_json::from_value::<CityCommand>(serde_json::json!({
                "kind": "advanceFixedSteps",
                "steps": 4
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<CityCommand>(serde_json::json!({
                "kind": "evaluateProgression"
            }))
            .is_err()
        );
    }

    #[test]
    fn disabled_planning_rejects_write_without_mutation() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.ruleset
            .set_status(RuleSystem::Planning, RuleStatus::Disabled);
        let before = save.clone();

        assert_eq!(
            save.execute(CityCommand::Planning {
                command: PlanningCommand::RemovePlayerRoad {
                    id: "player/road/1".to_owned(),
                },
            }),
            Err(CityCommandError::SystemDisabled(RuleSystem::Planning))
        );
        assert_eq!(save, before);
    }

    #[test]
    fn restart_does_not_seed_population_when_population_is_disabled() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.ruleset
            .set_status(RuleSystem::Population, RuleStatus::Disabled);
        save.ruleset
            .population
            .config
            .residential_floor_area_m2_per_household = 72;
        save.world.tick = 7;

        assert_eq!(
            save.execute(CityCommand::Restart).unwrap(),
            CityCommandOutcome::Restarted
        );
        assert_eq!(save.world, CityWorld::default());
        assert_eq!(
            save.ruleset
                .population
                .config
                .residential_floor_area_m2_per_household,
            72
        );
    }
}
