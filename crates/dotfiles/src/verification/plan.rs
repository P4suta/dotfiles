use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::source;
use rules::verification_rules as core;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Method {
    Test,
    Proof,
    Mutation,
    Fault,
    Fuzz,
    Model,
    Memory,
    Stress,
}

impl Method {
    pub(super) const fn core(self) -> core::Method {
        match self {
            Self::Test => core::Method::Test,
            Self::Proof => core::Method::Proof,
            Self::Mutation => core::Method::Mutation,
            Self::Fault => core::Method::Fault,
            Self::Fuzz => core::Method::Fuzz,
            Self::Model => core::Method::Model,
            Self::Memory => core::Method::Memory,
            Self::Stress => core::Method::Stress,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Risk {
    Pure,
    Input,
    State,
    Effects,
    Concurrency,
    Unsafe,
}

impl Risk {
    const fn core(self) -> core::Risk {
        match self {
            Self::Pure => core::Risk::Pure,
            Self::Input => core::Risk::Input,
            Self::State => core::Risk::State,
            Self::Effects => core::Risk::Effects,
            Self::Concurrency => core::Risk::Concurrency,
            Self::Unsafe => core::Risk::Unsafe,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Change {
    Behavior,
    Refactor,
    Configuration,
    Verification,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Scope {
    #[default]
    Affected,
    Full,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Tool {
    Cargo,
    Just,
    Dotfiles,
}

impl Tool {
    pub(super) const fn program(self) -> &'static str {
        match self {
            Self::Cargo => "cargo",
            Self::Just => "just",
            Self::Dotfiles => "dotfiles",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Report {
    Libtest,
    Kani,
    Mutants,
    Structured,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Check {
    pub id: String,
    pub method: Method,
    pub tool: Tool,
    pub arguments: Vec<String>,
    pub report: Report,
    pub expected: Vec<String>,
    #[serde(default)]
    pub rejecting: Vec<String>,
    pub report_path: Option<String>,
    pub verifier_version: Option<String>,
    #[serde(default)]
    pub environment: Vec<String>,
    #[serde(default)]
    pub scope: Scope,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Contract {
    pub path: String,
    pub change: Change,
    pub risks: Vec<Risk>,
    pub checks: Vec<String>,
}

impl Check {
    pub(super) fn identity(&self) -> Result<String> {
        Ok(source::digest(&serde_json::to_vec(self)?))
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            !self.id.is_empty()
                && self
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
            "unsafe verification check ID"
        );

        ensure!(
            !self.arguments.is_empty() && self.arguments.iter().all(|arg| !arg.contains('\0')),
            "verification command is empty or malformed"
        );
        ensure!(
            !self.expected.is_empty()
                && self
                    .expected
                    .iter()
                    .all(|subject| !subject.trim().is_empty()),
            "verification inventory is empty"
        );
        ensure!(
            self.expected.iter().collect::<BTreeSet<_>>().len() == self.expected.len(),
            "verification inventory is duplicated"
        );
        ensure!(
            self.report_path.as_deref().is_none_or(source::safe_path),
            "verification report path escapes the repository"
        );
        self.check_command()?;
        ensure!(
            self.environment.iter().all(|name| !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')),
            "invalid verification environment variable name"
        );
        match self.report {
            Report::Libtest => ensure!(
                matches!(self.method, Method::Test | Method::Fault | Method::Stress),
                "libtest cannot establish this verification method"
            ),
            Report::Kani => {
                ensure!(
                    self.method == Method::Proof && !self.rejecting.is_empty(),
                    "proof needs its rejecting inventory"
                );
                ensure!(
                    self.rejecting.iter().collect::<BTreeSet<_>>().len() == self.rejecting.len(),
                    "rejecting inventory is duplicated"
                );
            }
            Report::Mutants => ensure!(
                self.method == Method::Mutation
                    && self.report_path.is_some()
                    && self
                        .verifier_version
                        .as_deref()
                        .is_some_and(|version| !version.trim().is_empty()),
                "mutation needs its native report path and pinned verifier version"
            ),
            Report::Structured => {}
        }
        Ok(())
    }
}

impl Check {
    fn check_command(&self) -> Result<()> {
        let command = self
            .arguments
            .first()
            .map(String::as_str)
            .unwrap_or_default();
        let named = |value: &str| {
            matches!(
                value,
                "check"
                    | "test"
                    | "lint"
                    | "typecheck"
                    | "ci"
                    | "proofs"
                    | "mutants"
                    | "fuzz"
                    | "model"
                    | "miri"
                    | "stress"
                    | "verification"
            )
        };
        let permitted = match self.tool {
            Tool::Cargo => {
                matches!(command, "test" | "check" | "clippy" | "mutants" | "kani")
                    || command == "xtask" && self.arguments.get(1).is_some_and(|value| named(value))
            }
            Tool::Just => named(command),
            Tool::Dotfiles => {
                self.arguments
                    .starts_with(&["bounded".into(), "run".into()])
                    && self
                        .arguments
                        .iter()
                        .position(|value| value == "--")
                        .is_some_and(|index| index + 1 < self.arguments.len())
            }
        };
        ensure!(
            permitted,
            "verification runner permits named project checks only"
        );
        if matches!(self.report, Report::Kani) {
            ensure!(
                matches!(self.tool, Tool::Just) && command == "proofs"
                    || matches!(self.tool, Tool::Cargo)
                        && self
                            .arguments
                            .starts_with(&["xtask".into(), "proofs".into()]),
                "Kani evidence requires the maintained proof entry point"
            );
        }
        Ok(())
    }
}

impl Contract {
    pub(super) fn required(&self) -> u16 {
        self.risks
            .iter()
            .fold(core::Method::Test.bit(), |mask, risk| {
                mask | core::required(risk.core())
            })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Plan {
    pub version: u8,
    pub contracts: Vec<Contract>,
    pub checks: Vec<Check>,
}

impl Plan {
    pub(super) fn extends(&self, previous: &Self) -> Result<bool> {
        for old in &previous.checks {
            let Some(current) = self.checks.iter().find(|check| check.id == old.id) else {
                return Ok(false);
            };
            let mut definition = current.clone();
            definition.expected.clone_from(&old.expected);
            if !rules::verification_rules::coverage_extension(
                serde_json::to_vec(&definition)? == serde_json::to_vec(old)?,
                old.expected
                    .iter()
                    .all(|subject| current.expected.contains(subject)),
            ) {
                return Ok(false);
            }
        }
        Ok(previous.contracts.iter().all(|old| {
            self.contracts.iter().any(|current| {
                current.path == old.path
                    && current.change == old.change
                    && old.risks.iter().all(|risk| current.risks.contains(risk))
                    && old.checks.iter().all(|id| current.checks.contains(id))
            })
        }))
    }

    pub(super) fn identity(&self) -> Result<String> {
        Ok(source::digest(&serde_json::to_vec(self)?))
    }

    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            self.version == super::VERSION,
            "unsupported verification plan version"
        );
        ensure!(
            !self.contracts.is_empty() && !self.checks.is_empty(),
            "verification plan has no contracts or checks"
        );
        let mut checks = BTreeMap::new();
        for check in &self.checks {
            check.validate()?;
            ensure!(
                checks.insert(check.id.as_str(), check).is_none(),
                "duplicate verification check ID"
            );
        }
        let mut paths = BTreeSet::new();
        for contract in &self.contracts {
            ensure!(
                source::safe_path(&contract.path) && paths.insert(&contract.path),
                "contract path is unsafe or duplicated"
            );
            ensure!(
                contract.change != Change::Behavior || !contract.risks.is_empty(),
                "behavior contract needs its risk assessment"
            );
            ensure!(
                contract.change != Change::Verification || contract.risks.is_empty(),
                "verification-only contract cannot hide behavior risks"
            );
            let mut methods = 0;
            for id in &contract.checks {
                let check = checks
                    .get(id.as_str())
                    .ok_or_else(|| anyhow::anyhow!("contract names missing check {id}"))?;
                methods |= check.method.core().bit();
            }
            ensure!(
                methods & contract.required() == contract.required(),
                "contract {} omits an applicable verification method",
                contract.path
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_inventory_may_only_grow_without_changing_the_check() -> Result<()> {
        let original = serde_json::json!({"version":1,"contracts":[{"path":"src","change":"behavior","risks":["pure"],"checks":["test"]}],"checks":[{"id":"test","method":"test","tool":"cargo","arguments":["test"],"report":"libtest","expected":["existing"]}]});
        let previous: Plan = serde_json::from_value(original.clone())?;
        assert!(previous.extends(&previous)?);
        let mut expanded = original.clone();
        expanded["checks"][0]["expected"] = serde_json::json!(["existing", "additional"]);
        assert!(serde_json::from_value::<Plan>(expanded.clone())?.extends(&previous)?);
        for (field, value) in [
            ("expected", serde_json::json!(["additional"])),
            ("arguments", serde_json::json!(["test", "--ignored"])),
            ("report_path", "elsewhere.json".into()),
            ("rejecting", serde_json::json!(["different"])),
            ("environment", serde_json::json!(["DIFFERENT"])),
            ("scope", "full".into()),
        ] {
            let mut changed = expanded.clone();
            changed["checks"][0][field] = value;
            assert!(
                !serde_json::from_value::<Plan>(changed)?.extends(&previous)?,
                "{field}"
            );
        }
        expanded["contracts"][0]["risks"] = serde_json::json!([]);
        assert!(!serde_json::from_value::<Plan>(expanded)?.extends(&previous)?);
        Ok(())
    }

    #[test]
    fn bounded_verification_runs_cannot_choose_installation_or_unbounded_dispatch() {
        let mut check: Check = serde_json::from_value(serde_json::json!({
            "id": "bounded-tests",
            "method": "test",
            "tool": "dotfiles",
            "arguments": ["bounded", "run", "--image", "sha256:5831cd854a776d2ef8ee586b5275111d81a7b94671494ecc1c418f2c3d6a13e0", "--", "cargo", "test"],
            "report": "libtest",
            "expected": ["contract"]
        }))
        .expect("the maintained bounded runner is a supported verification tool");
        check.validate().expect("a bounded test command is valid");
        for arguments in [
            vec!["bounded", "install"],
            vec!["mise-install", "rust"],
            vec!["assure", "check"],
            vec!["bounded", "run"],
            vec!["bounded", "run", "--"],
        ] {
            check.arguments = arguments.into_iter().map(str::to_owned).collect();
            assert!(check.validate().is_err());
        }
    }
}
