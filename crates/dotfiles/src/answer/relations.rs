use anyhow::{Result, ensure};
use rules::answer_rules::{Preference, Ranking, rank};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Objective,
    Principle,
    Method,
    Mechanism,
    Implementation,
    Observation,
    Outcome,
    Limitation,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Axis {
    role: Role,
    metric: String,
    population: String,
    period: String,
    unit: String,
    denominator: String,
    conditions: String,
}

impl Axis {
    fn validate(&self) -> Result<()> {
        ensure!(
            [
                &self.metric,
                &self.population,
                &self.period,
                &self.unit,
                &self.denominator,
                &self.conditions
            ]
            .into_iter()
            .all(|field| !field.trim().is_empty()),
            "comparison axis omits a material scope or measurement field"
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OptionValue {
    label: String,
    basis: String,
    axis: Axis,
    value: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Comparison {
    first: OptionValue,
    second: OptionValue,
    #[serde(with = "PreferenceDef")]
    preference: Preference,
    winner: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(remote = "Preference", rename_all = "kebab-case")]
enum PreferenceDef {
    Higher,
    Lower,
}

impl Comparison {
    pub fn bases(&self) -> [&str; 2] {
        [&self.first.basis, &self.second.basis]
    }

    pub fn check(&self, text: &str) -> Result<()> {
        self.first.axis.validate()?;
        self.second.axis.validate()?;
        ensure!(
            self.first.axis == self.second.axis,
            "comparison mixes abstraction levels, metrics, populations, periods, units, denominators, or conditions"
        );
        ensure!(
            !self.first.label.is_empty()
                && !self.second.label.is_empty()
                && self.first.label != self.second.label,
            "comparison needs two distinct alternatives"
        );
        ensure!(
            text.contains(&self.first.label) && text.contains(&self.second.label),
            "comparison metadata names alternatives absent from the answer"
        );
        if let Some(winner) = &self.winner {
            let (Some(first), Some(second)) = (self.first.value, self.second.value) else {
                anyhow::bail!("comparison cannot rank an unknown measurement");
            };
            let expected = match rank(first, second, self.preference) {
                Ranking::First => Some(&self.first.label),
                Ranking::Second => Some(&self.second.label),
                Ranking::Equal => None,
            };
            ensure!(
                expected == Some(winner),
                "comparison winner contradicts its declared values or preference"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParallelItem {
    text: String,
    role: Role,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Parallel {
    role: Role,
    items: Vec<ParallelItem>,
}

impl Parallel {
    pub fn check(&self, text: &str) -> Result<()> {
        ensure!(
            self.items.len() >= 2,
            "parallel relation needs at least two items"
        );
        let mut seen = BTreeSet::new();
        for item in &self.items {
            ensure!(
                item.role == self.role,
                "parallel relation mixes semantic roles or abstraction levels"
            );
            ensure!(
                !item.text.is_empty() && seen.insert(&item.text) && text.contains(&item.text),
                "parallel item is empty, duplicated, or absent from this answer"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(label: &str, value: i64) -> OptionValue {
        OptionValue {
            label: label.to_owned(),
            basis: "observation".to_owned(),
            axis: Axis {
                role: Role::Implementation,
                metric: "latency".to_owned(),
                population: "same workload".to_owned(),
                period: "same run".to_owned(),
                unit: "ms".to_owned(),
                denominator: "request".to_owned(),
                conditions: "same machine".to_owned(),
            },
            value: Some(value),
        }
    }

    #[test]
    fn a_comparison_needs_matched_axes_and_a_correct_ranking() {
        let mut comparison = Comparison {
            first: option("A", 1),
            second: option("B", 2),
            preference: Preference::Lower,
            winner: Some("A".to_owned()),
        };
        assert!(comparison.check("A has lower latency than B.").is_ok());
        comparison.winner = Some("B".to_owned());
        assert!(comparison.check("B has lower latency than A.").is_err());
        comparison.winner = None;
        let axis = comparison.second.axis.clone();
        for mismatch in 0..7 {
            comparison.second.axis = axis.clone();
            match mismatch {
                0 => comparison.second.axis.role = Role::Objective,
                1 => comparison.second.axis.metric = "throughput".to_owned(),
                2 => comparison.second.axis.population = "other workload".to_owned(),
                3 => comparison.second.axis.period = "another period".to_owned(),
                4 => comparison.second.axis.unit = "seconds".to_owned(),
                5 => comparison.second.axis.denominator = "batch".to_owned(),
                _ => comparison.second.axis.conditions = "another machine".to_owned(),
            }
            assert!(comparison.check("Compare A and B.").is_err(), "{mismatch}");
        }
        comparison.second.axis = axis;
        comparison.first.value = None;
        comparison.winner = Some("B".to_owned());
        assert!(comparison.check("Compare A and B.").is_err());
    }

    #[test]
    fn a_parallel_relation_cannot_mix_a_goal_with_its_implementation() {
        let relation = Parallel {
            role: Role::Objective,
            items: vec![
                ParallelItem {
                    text: "Reliable service".to_owned(),
                    role: Role::Objective,
                },
                ParallelItem {
                    text: "Use a cache".to_owned(),
                    role: Role::Implementation,
                },
            ],
        };
        assert!(relation.check("Reliable service and Use a cache").is_err());
    }
}
