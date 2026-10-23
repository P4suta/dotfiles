use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt::Write as _;

const MAX_ATOMS: usize = 8;
const MAX_FORMULA_NODES: usize = 64;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(
    tag = "op",
    content = "args",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum Formula {
    Atom(u8),
    Not(Box<Self>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
    Implies(Box<Self>, Box<Self>),
}

impl Formula {
    fn nodes(&self) -> usize {
        match self {
            Self::Atom(_) => 1,
            Self::Not(child) => 1 + child.nodes(),
            Self::And(left, right) | Self::Or(left, right) | Self::Implies(left, right) => {
                1 + left.nodes() + right.nodes()
            }
        }
    }

    fn evaluate(&self, assignment: u16, atoms: usize) -> Result<bool> {
        Ok(match self {
            Self::Atom(index) => {
                ensure!(
                    usize::from(*index) < atoms,
                    "formula names an undeclared atom"
                );
                assignment & (1 << index) != 0
            }
            Self::Not(child) => !child.evaluate(assignment, atoms)?,
            Self::And(left, right) => {
                left.evaluate(assignment, atoms)? & right.evaluate(assignment, atoms)?
            }
            Self::Or(left, right) => {
                left.evaluate(assignment, atoms)? | right.evaluate(assignment, atoms)?
            }
            Self::Implies(left, right) => {
                !left.evaluate(assignment, atoms)? | right.evaluate(assignment, atoms)?
            }
        })
    }

    fn lean(&self) -> String {
        match self {
            Self::Atom(index) => format!("p{index}"),
            Self::Not(child) => format!("(!{})", child.lean()),
            Self::And(left, right) => format!("({} && {})", left.lean(), right.lean()),
            Self::Or(left, right) => format!("({} || {})", left.lean(), right.lean()),
            Self::Implies(left, right) => format!("(!{} || {})", left.lean(), right.lean()),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Argument {
    pub premises: Vec<String>,
    pub conclusion: Formula,
}

/// Refuse a countermodel before submitting the generated theorem to Lean.
pub fn theorem(
    index: usize,
    atoms: &[String],
    premises: &[&Formula],
    conclusion: &Formula,
) -> Result<String> {
    ensure!(
        !atoms.is_empty() && atoms.len() <= MAX_ATOMS,
        "an argument needs 1..={MAX_ATOMS} declared atoms"
    );
    ensure!(
        atoms.iter().all(|atom| !atom.trim().is_empty()),
        "an atom has no interpretation"
    );
    ensure!(
        atoms.iter().collect::<BTreeSet<_>>().len() == atoms.len(),
        "duplicate atom interpretations"
    );
    for formula in premises.iter().copied().chain(std::iter::once(conclusion)) {
        ensure!(
            formula.nodes() <= MAX_FORMULA_NODES,
            "formula exceeds its structural bound"
        );
    }
    let assignments = 1_u16 << atoms.len();
    let mut satisfiable = false;
    let mut equivalent = vec![true; premises.len()];
    for assignment in 0..assignments {
        let conclusion = conclusion.evaluate(assignment, atoms.len())?;
        let values = premises
            .iter()
            .map(|premise| premise.evaluate(assignment, atoms.len()))
            .collect::<Result<Vec<_>>>()?;
        for (same, value) in equivalent.iter_mut().zip(&values) {
            *same &= *value == conclusion;
        }
        if values.into_iter().all(core::convert::identity) {
            satisfiable = true;
            if !conclusion {
                bail!("argument {index} has a countermodel: assignment {assignment}");
            }
        }
    }
    ensure!(satisfiable, "argument {index} has inconsistent premises");
    ensure!(
        !equivalent.contains(&true),
        "argument {index} assumes its conclusion"
    );
    let mut result = format!("theorem argument{index} : ∀ ");
    for atom in 0..atoms.len() {
        write!(result, "(p{atom} : Bool) ")?;
    }
    result.push_str(", ");
    for premise in premises {
        write!(result, "({} = true) → ", premise.lean())?;
    }
    writeln!(result, "({} = true) := by decide", conclusion.lean())?;
    writeln!(result, "#print axioms argument{index}")?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Formula::{And, Atom, Implies, Not};

    #[test]
    fn invalid_arguments_cannot_reach_the_prover() {
        let p = Atom(0);
        let q = Atom(1);
        let implication = Implies(Box::new(p.clone()), Box::new(q.clone()));
        let atoms = vec![
            "The input passes".to_owned(),
            "The output passes".to_owned(),
        ];
        assert!(theorem(0, &atoms, &[&p, &implication], &q).is_ok());
        for premises in [
            vec![&q, &implication],
            vec![&q],
            vec![&p, &Not(Box::new(p.clone()))],
        ] {
            assert!(theorem(0, &atoms, &premises, &q).is_err());
        }
        assert!(
            theorem(
                0,
                &atoms,
                &[&p],
                &And(Box::new(p.clone()), Box::new(p.clone()))
            )
            .is_err()
        );
        assert!(theorem(0, &atoms, &[], &Atom(2)).is_err());
    }

    #[test]
    fn interpretations_cannot_inject_lean_code() {
        let source = theorem(
            2,
            &["axiom false : False".to_owned()],
            &[],
            &Implies(Box::new(Atom(0)), Box::new(Atom(0))),
        )
        .unwrap();
        assert!(!source.contains("axiom false"));
        assert!(source.contains("∀ (p0 : Bool)"));
    }
}
