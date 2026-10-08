//! What a script's `# inputs:` hash may read, so the hash changes only when the step's own inputs change.

extern crate alloc;
use self::alloc::string::String;
use self::alloc::vec::Vec;

/// One term of the expression a script passes to the `inputs` partial.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Term {
    /// A data path such as `.ocaml`, a literal, a partial name, or a pure function.
    Data,
    /// `.` or `$`, the whole template context, which holds every profile's data and the chezmoi command line.
    Context,
    /// A `.chezmoi` variable, which the host and the command line set.
    Chezmoi,
    /// A function that reads the host, such as `output` or `stat`.
    HostRead,
    /// `includeTemplate`, whose second term after it may pass the context to the partial it renders.
    Render,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    HostState,
    WholeContext,
}

/// How many terms before the context argument `includeTemplate` stands, with the partial's name between them.
const CONTEXT_AFTER_RENDER: usize = 2;

/// A host-reading term refuses the expression outright.
/// The whole context passes only as the context argument of a rendered partial, as in `includeTemplate "name" .`.
#[must_use]
pub fn verdict(terms: &[Term]) -> Verdict {
    let mut whole = false;
    let mut rest = terms;
    let mut index: usize = 0;
    while let [term, tail @ ..] = rest {
        let term = *term;
        rest = tail;
        match term {
            Term::Chezmoi | Term::HostRead => return Verdict::HostState,
            Term::Context
                if !matches!(
                    index
                        .checked_sub(CONTEXT_AFTER_RENDER)
                        .and_then(|render| terms.get(render)),
                    Some(Term::Render)
                ) =>
            {
                whole = true;
            }
            Term::Context | Term::Data | Term::Render => {}
        }
        index += 1;
    }
    if whole {
        Verdict::WholeContext
    } else {
        Verdict::Admit
    }
}

/// Template functions that read the host or the environment instead of the source.
const HOST_READS: [&str; 11] = [
    "output",
    "stat",
    "lstat",
    "glob",
    "env",
    "lookPath",
    "findExecutable",
    "findOneExecutable",
    "isExecutable",
    "exec",
    "outputList",
];

/// The term a token of a template expression stands for.
///
/// Exempt as disproportionate: Kani runs past its budget on the string comparisons of even a short token.
/// Evidence: `crates/rules/src/input_rules.rs::tests::each_token_names_its_term`.
/// Reconsider when a cargo-fuzz target owns this function.
#[must_use]
pub fn term(token: &str) -> Term {
    match token {
        "." | "$" => Term::Context,
        "includeTemplate" => Term::Render,
        _ if HOST_READS.contains(&token) => Term::HostRead,
        _ if token.starts_with(".chezmoi") || token.starts_with("$.chezmoi") => Term::Chezmoi,
        _ => Term::Data,
    }
}

/// The terms of a template expression, split at whitespace and parentheses, with each quoted string as one term.
///
/// Exempt as disproportionate: a parser over unbounded text needs fuzzing, which no verifier here owns yet.
/// Evidence: `crates/rules/src/input_rules.rs::tests::terms_split_at_spaces_and_parentheses_and_keep_quotes_whole`.
/// Reconsider when a cargo-fuzz target owns this function.
#[must_use]
pub fn terms(expression: &str) -> Vec<Term> {
    let mut found = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in expression.chars() {
        match character {
            '"' => {
                current.push(character);
                quoted = !quoted;
            }
            _ if quoted => current.push(character),
            '(' | ')' => {
                if !current.is_empty() {
                    found.push(term(&current));
                    current.clear();
                }
            }
            _ if character.is_whitespace() => {
                if !current.is_empty() {
                    found.push(term(&current));
                    current.clear();
                }
            }
            _ => current.push(character),
        }
    }
    if !current.is_empty() {
        found.push(term(&current));
    }
    found
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const TERMS: [Term; 5] = [
        Term::Data,
        Term::Context,
        Term::Chezmoi,
        Term::HostRead,
        Term::Render,
    ];

    /// A host-reading term refuses the hash, and otherwise only a context outside a rendered partial does.
    pub fn spec_verdict(terms: &[Term]) {
        let mut host = false;
        let mut bare = false;
        let mut index = 0;
        while let Some(&term) = terms.get(index) {
            host |= matches!(term, Term::Chezmoi | Term::HostRead);
            bare |= term == Term::Context
                && !(index >= 2 && terms.get(index - 2) == Some(&Term::Render));
            index += 1;
        }
        let result = verdict(terms);
        assert_eq!(result == Verdict::HostState, host);
        assert_eq!(result == Verdict::WholeContext, !host && bare);
        assert_eq!(result == Verdict::Admit, !host && !bare);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn a_host_read_refuses_the_hash_and_the_whole_context_passes_only_into_a_rendered_partial() {
        let terms = [
            TERMS[usize::from(kani::any::<u8>() % 5)],
            TERMS[usize::from(kani::any::<u8>() % 5)],
            TERMS[usize::from(kani::any::<u8>() % 5)],
            TERMS[usize::from(kani::any::<u8>() % 5)],
        ];
        let length: usize = kani::any();
        kani::assume(length <= terms.len());
        let terms = &terms[..length];
        spec_verdict(terms);
        let result = verdict(terms);
        kani::cover!(result == Verdict::HostState);
        kani::cover!(result == Verdict::WholeContext);
        kani::cover!(result == Verdict::Admit && length == 3 && terms[2] == Term::Context);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every expression")]
    fn every_expression_of_up_to_four_terms_meets_the_specification() {
        let mut expressions: Vec<Vec<Term>> = alloc::vec![Vec::new()];
        for _ in 0..4 {
            let longer: Vec<Vec<Term>> = expressions
                .iter()
                .filter(|terms| terms.len() == expressions.last().map_or(0, Vec::len))
                .flat_map(|terms| {
                    proofs::TERMS.iter().map(move |&term| {
                        let mut next = terms.clone();
                        next.push(term);
                        next
                    })
                })
                .collect();
            expressions.extend(longer);
        }
        for terms in &expressions {
            proofs::spec_verdict(terms);
        }
    }

    #[test]
    fn each_token_names_its_term() {
        assert_eq!(term("."), Term::Context);
        assert_eq!(term("$"), Term::Context);
        assert_eq!(term("includeTemplate"), Term::Render);
        for read in HOST_READS {
            assert_eq!(term(read), Term::HostRead, "{read}");
        }
        assert_eq!(term(".chezmoi.os"), Term::Chezmoi);
        assert_eq!(term("$.chezmoi.os"), Term::Chezmoi);
        assert_eq!(term(".chezmoidata"), Term::Chezmoi);
        assert_eq!(term(".ocaml"), Term::Data);
        assert_eq!(term("list"), Term::Data);
    }

    #[test]
    fn terms_split_at_spaces_and_parentheses_and_keep_quotes_whole() {
        assert_eq!(terms(""), []);
        assert_eq!(terms("  "), []);
        assert_eq!(terms("(.a)"), [Term::Data]);
        assert_eq!(terms(".a\t."), [Term::Data, Term::Context]);
        assert_eq!(terms("\"x y\" ."), [Term::Data, Term::Context]);
        assert_eq!(terms("(output)(.)"), [Term::HostRead, Term::Context]);
    }

    #[test]
    fn a_step_hashes_its_own_data_and_rendered_partials() {
        for expression in [
            "(list)",
            "(list .ocaml)",
            "(list (index .tool_versions \"haskell-language-server\"))",
            "(list (includeTemplate \"native/mise/mac.toml.tmpl\" .) .tools)",
            "(list (include \"dot_config/dotfiles-nix/flake.nix.tmpl\") .nix)",
        ] {
            assert_eq!(verdict(&terms(expression)), Verdict::Admit, "{expression}");
        }
    }

    #[test]
    fn the_whole_context_and_host_reads_are_refused() {
        assert_eq!(verdict(&terms(".")), Verdict::WholeContext);
        assert_eq!(verdict(&terms("(list . .ocaml)")), Verdict::WholeContext);
        assert_eq!(
            verdict(&terms("(list .chezmoi.sourceDir)")),
            Verdict::HostState
        );
        assert_eq!(
            verdict(&terms("(list (output \"git\" \"rev-parse\" \"HEAD\"))")),
            Verdict::HostState
        );
        assert_eq!(
            verdict(&terms("(list (stat \"/tmp\"))")),
            Verdict::HostState
        );
    }

    #[test]
    fn a_quoted_name_is_one_data_term() {
        assert_eq!(
            terms("includeTemplate \"a (b) c\" ."),
            [Term::Render, Term::Data, Term::Context]
        );
    }
}
