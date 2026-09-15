use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{CitySave, RuleSystem, RulesetError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Requirement {
    MetricAtLeast { metric: String, value: i64 },
    SystemUnlocked { system: String },
}

impl Requirement {
    fn is_met(&self, metrics: &BTreeMap<String, i64>, unlocked: &BTreeSet<String>) -> bool {
        match self {
            Self::MetricAtLeast { metric, value } => {
                metrics.get(metric).copied().unwrap_or_default() >= *value
            }
            Self::SystemUnlocked { system } => unlocked.contains(system),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressionRule {
    pub id: String,
    pub unlocks: String,
    #[serde(default)]
    pub all: Vec<Requirement>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressionState {
    #[serde(default)]
    pub unlocked: BTreeSet<String>,
}

impl ProgressionState {
    /// Evaluates rules to a fixed point. Each wave reads one immutable snapshot so the result does
    /// not depend on the caller's rule ordering.
    pub fn evaluate(
        &mut self,
        rules: &[ProgressionRule],
        metrics: &BTreeMap<String, i64>,
    ) -> Vec<String> {
        let mut ordered = rules.iter().collect::<Vec<_>>();
        ordered.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then(left.unlocks.cmp(&right.unlocks))
        });
        let mut newly_unlocked = Vec::new();

        loop {
            let snapshot = self.unlocked.clone();
            let mut wave = ordered
                .iter()
                .copied()
                .filter(|rule| !snapshot.contains(&rule.unlocks))
                .filter(|rule| {
                    rule.all
                        .iter()
                        .all(|requirement| requirement.is_met(metrics, &snapshot))
                })
                .map(|rule| rule.unlocks.clone())
                .collect::<Vec<_>>();
            wave.sort();
            wave.dedup();

            if wave.is_empty() {
                break;
            }
            for system in wave {
                if self.unlocked.insert(system.clone()) {
                    newly_unlocked.push(system);
                }
            }
        }

        newly_unlocked
    }
}

impl CitySave {
    /// Runs the progression simulation system directly against authoritative city state.
    ///
    /// Progression is an internal deterministic system, not an application command. A disabled
    /// progression system is therefore skipped rather than rejected as invalid player intent.
    pub fn evaluate_progression(&mut self) -> Result<Vec<String>, RulesetError> {
        self.ruleset.validate()?;
        if !self.ruleset.is_enabled(RuleSystem::Progression) {
            return Ok(Vec::new());
        }

        let rules = self.ruleset.progression.config.rules.clone();
        Ok(self
            .world
            .progression
            .evaluate(&rules, &self.world.metrics))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        CityScenario, ExternalRevision, RuleStatus, SCENARIO_SCHEMA_VERSION, ScenarioProvenance,
    };

    use super::*;

    fn rule(id: &str, unlocks: &str, all: Vec<Requirement>) -> ProgressionRule {
        ProgressionRule {
            id: id.to_owned(),
            unlocks: unlocks.to_owned(),
            all,
        }
    }

    fn scenario() -> CityScenario {
        CityScenario {
            schema_version: SCENARIO_SCHEMA_VERSION,
            provenance: ScenarioProvenance {
                source_format: "fixture".to_owned(),
                source_name: "progression".to_owned(),
                source_sha256: "progression".to_owned(),
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
    fn progression_is_order_independent_and_reaches_a_fixed_point() {
        let metrics = BTreeMap::from([("population".to_owned(), 1_000)]);
        let rules = vec![
            rule(
                "waste",
                "waste-management",
                vec![Requirement::SystemUnlocked {
                    system: "basic-services".to_owned(),
                }],
            ),
            rule(
                "services",
                "basic-services",
                vec![Requirement::MetricAtLeast {
                    metric: "population".to_owned(),
                    value: 1_000,
                }],
            ),
        ];

        let mut forward = ProgressionState::default();
        let mut reverse = ProgressionState::default();
        forward.evaluate(&rules, &metrics);
        reverse.evaluate(&rules.iter().cloned().rev().collect::<Vec<_>>(), &metrics);

        assert_eq!(forward, reverse);
        assert!(forward.unlocked.contains("basic-services"));
        assert!(forward.unlocked.contains("waste-management"));
        assert!(forward.evaluate(&rules, &metrics).is_empty());
    }

    #[test]
    fn unmet_metric_keeps_later_responsibility_locked() {
        let metrics = BTreeMap::from([("population".to_owned(), 999)]);
        let rules = [rule(
            "waste",
            "waste-management",
            vec![Requirement::MetricAtLeast {
                metric: "population".to_owned(),
                value: 1_000,
            }],
        )];
        let mut state = ProgressionState::default();

        assert!(state.evaluate(&rules, &metrics).is_empty());
        assert!(!state.unlocked.contains("waste-management"));
    }

    #[test]
    fn save_progression_evaluation_uses_ruleset_and_world_metrics() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.world.metrics.insert("population".to_owned(), 1_000);
        save.ruleset.progression.config.rules = vec![rule(
            "services",
            "basic-services",
            vec![Requirement::MetricAtLeast {
                metric: "population".to_owned(),
                value: 1_000,
            }],
        )];

        assert_eq!(
            save.evaluate_progression().unwrap(),
            vec!["basic-services".to_owned()]
        );
        assert!(save.world.progression.unlocked.contains("basic-services"));
        assert!(save.evaluate_progression().unwrap().is_empty());
    }

    #[test]
    fn disabled_progression_is_a_simulation_noop() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.ruleset
            .set_status(RuleSystem::Progression, RuleStatus::Disabled);
        save.ruleset.progression.config.rules =
            vec![rule("services", "basic-services", Vec::new())];
        let before = save.clone();

        assert!(save.evaluate_progression().unwrap().is_empty());
        assert_eq!(save, before);
    }
}
