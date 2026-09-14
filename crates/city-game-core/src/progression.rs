use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, unlocks: &str, all: Vec<Requirement>) -> ProgressionRule {
        ProgressionRule {
            id: id.to_owned(),
            unlocks: unlocks.to_owned(),
            all,
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
}
