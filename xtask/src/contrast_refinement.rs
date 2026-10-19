//! A checked translation of the production contrast arithmetic into Lean natural numbers.

use anyhow::{Context, Result, bail, ensure};
use quote::ToTokens;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use syn::{BinOp, Expr, FnArg, Item, Pat, ReturnType, Stmt};

pub const SOURCE: &str = "crates/rules/src/contrast_rules.rs";
pub const GENERATED: &str = "specs/lean/Dotfiles/ContrastSource.lean";
const LUMINANCES: usize = 2;
const CONSTANTS: [&str; 7] = [
    "FLARE",
    "LEAST_NUMERATOR",
    "LEAST_DENOMINATOR",
    "MILLIS",
    "MAX_LINEAR",
    "MAX_LUMINANCE",
    "MAX_RATIO",
];
const FUNCTIONS: [&str; 4] = [
    "ordered_luminances",
    "keeps_least_contrast",
    "ratio_ordered",
    "ratio_luminances",
];

fn pattern(binding: &Pat, scope: &mut BTreeSet<String>) -> Result<String> {
    match binding {
        Pat::Ident(binding) if binding.by_ref.is_none() && binding.subpat.is_none() => {
            let name = binding.ident.to_string();
            scope.insert(name.clone());
            Ok(name)
        }
        Pat::Tuple(tuple) => Ok(format!(
            "({})",
            tuple
                .elems
                .iter()
                .map(|item| pattern(item, scope))
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        )),
        _ => bail!(
            "unsupported contrast binding: {}",
            binding.to_token_stream()
        ),
    }
}

fn expression(expr: &Expr, scope: &BTreeSet<String>) -> Result<String> {
    match expr {
        Expr::Lit(literal) => {
            let syn::Lit::Int(value) = &literal.lit else {
                bail!("contrast literals must be unsigned integers");
            };
            ensure!(
                value.suffix().is_empty() || value.suffix() == "u64",
                "contrast literal has a different integer type"
            );
            Ok(value.base10_parse::<u64>()?.to_string())
        }
        Expr::Path(path) if path.qself.is_none() => {
            let name = path
                .path
                .get_ident()
                .context("contrast paths must be local identifiers")?
                .to_string();
            ensure!(
                scope.contains(&name),
                "untranslated contrast identifier {name}"
            );
            Ok(name)
        }
        Expr::Paren(paren) => expression(&paren.expr, scope),
        Expr::Tuple(tuple) => Ok(format!(
            "({})",
            tuple
                .elems
                .iter()
                .map(|item| expression(item, scope))
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        )),
        Expr::Binary(binary) => {
            let left = expression(&binary.left, scope)?;
            let right = expression(&binary.right, scope)?;
            match binary.op {
                BinOp::Add(_) => Ok(format!("({left} + {right})")),
                BinOp::Mul(_) => Ok(format!("({left} * {right})")),
                BinOp::Div(_) => Ok(format!("({left} / {right})")),
                BinOp::Ge(_) => Ok(format!("(decide ({left} ≥ {right}))")),
                _ => bail!(
                    "unsupported contrast operator: {}",
                    binary.op.to_token_stream()
                ),
            }
        }
        Expr::Call(call) => {
            ensure!(
                call.args.len() == LUMINANCES,
                "contrast calls require two luminances"
            );
            let Expr::Path(path) = call.func.as_ref() else {
                bail!("contrast calls must name a translated function");
            };
            let name = path
                .path
                .get_ident()
                .context("contrast calls must be local")?
                .to_string();
            ensure!(
                FUNCTIONS.contains(&name.as_str()),
                "untranslated contrast call {name}"
            );
            Ok(format!(
                "({name} {})",
                call.args
                    .iter()
                    .map(|arg| expression(arg, scope))
                    .collect::<Result<Vec<_>>>()?
                    .join(" ")
            ))
        }
        Expr::If(branch) => {
            let condition = expression(&branch.cond, scope)?;
            let yes = block(&branch.then_branch, scope)?;
            let (_, no) = branch
                .else_branch
                .as_ref()
                .context("contrast branches must be exhaustive")?;
            Ok(format!(
                "(if {condition} then {yes} else {})",
                expression(no, scope)?
            ))
        }
        Expr::Block(body) if body.label.is_none() => block(&body.block, scope),
        _ => bail!(
            "unsupported contrast expression: {}",
            expr.to_token_stream()
        ),
    }
}

fn block(body: &syn::Block, scope: &BTreeSet<String>) -> Result<String> {
    let mut scope = scope.clone();
    let mut output = String::new();
    for (index, statement) in body.stmts.iter().enumerate() {
        match statement {
            Stmt::Local(local) if local.attrs.is_empty() => {
                let init = local
                    .init
                    .as_ref()
                    .context("contrast bindings require a value")?;
                ensure!(init.diverge.is_none(), "contrast bindings cannot diverge");
                let value = expression(&init.expr, &scope)?;
                let binding = pattern(&local.pat, &mut scope)?;
                write!(output, "let {binding} := {value}; ")?;
            }
            Stmt::Expr(value, None) if index + 1 == body.stmts.len() => {
                output.push_str(&expression(value, &scope)?);
                return Ok(output);
            }
            _ => bail!(
                "unsupported contrast statement: {}",
                statement.to_token_stream()
            ),
        }
    }
    bail!("contrast functions require a final expression")
}

/// Translates each admitted Rust expression and rejects every unmodeled operation.
/// Kani proves that these unsigned operations neither wrap nor divide by zero over all admitted luminances.
///
/// # Errors
///
/// Returns an error for missing declarations, a different type, or syntax outside the checked subset.
pub fn translate(source: &str) -> Result<String> {
    let file = syn::parse_file(source).context("parse the production contrast module")?;
    let mut output = String::from(
        "/-!\nGenerated by `just gen` from the production Rust contrast expressions.\nKani checks unsigned arithmetic safety, so these natural-number definitions preserve their values.\n-/\n\nnamespace Dotfiles.Contrast.Source\n\n",
    );
    let mut scope = BTreeSet::new();
    for name in CONSTANTS {
        let declarations: Vec<_> = file
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Const(item) if item.ident == name => Some(item),
                _ => None,
            })
            .collect();
        ensure!(
            declarations.len() == 1,
            "missing or duplicate contrast constant {name}"
        );
        let declaration = declarations[0];
        ensure!(
            declaration.ty.to_token_stream().to_string() == "u64",
            "contrast constant {name} must be u64"
        );
        writeln!(
            output,
            "def {name} : Nat := {}",
            expression(&declaration.expr, &scope)?
        )?;
        scope.insert(name.to_owned());
    }
    for name in FUNCTIONS {
        let declarations: Vec<_> = file
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Fn(item) if item.sig.ident == name => Some(item),
                _ => None,
            })
            .collect();
        ensure!(
            declarations.len() == 1,
            "missing or duplicate contrast function {name}"
        );
        let function = declarations[0];
        let signature = &function.sig;
        ensure!(
            signature.asyncness.is_none()
                && signature.unsafety.is_none()
                && signature.abi.is_none()
                && signature.generics.params.is_empty()
                && signature.generics.where_clause.is_none()
                && signature.variadic.is_none()
                && signature.inputs.len() == LUMINANCES,
            "contrast function {name} has an unsupported signature"
        );
        let mut local_scope = scope.clone();
        let mut parameters = Vec::new();
        for parameter in &signature.inputs {
            let FnArg::Typed(parameter) = parameter else {
                bail!("contrast functions cannot take self");
            };
            ensure!(
                parameter.attrs.is_empty() && parameter.ty.to_token_stream().to_string() == "u64",
                "contrast parameters must be u64"
            );
            let Pat::Ident(binding) = parameter.pat.as_ref() else {
                bail!("contrast parameters must be identifiers");
            };
            ensure!(
                binding.by_ref.is_none() && binding.subpat.is_none(),
                "contrast parameters must be values"
            );
            let parameter = binding.ident.to_string();
            local_scope.insert(parameter.clone());
            parameters.push(format!("({parameter} : Nat)"));
        }
        let ReturnType::Type(_, result) = &signature.output else {
            bail!("contrast functions require a return type");
        };
        let result = match result.to_token_stream().to_string().as_str() {
            "u64" => "Nat",
            "bool" => "Bool",
            "(u64 , u64)" => "Nat × Nat",
            other => bail!("unsupported contrast return type {other}"),
        };
        writeln!(
            output,
            "\ndef {name} {} : {result} :=\n  {}",
            parameters.join(" "),
            block(&function.block, &local_scope)?
        )?;
    }
    output.push_str("\nend Dotfiles.Contrast.Source\n");
    Ok(output)
}
