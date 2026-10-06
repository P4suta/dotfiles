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
    let mut index = 0;
    while let Some(&term) = terms.get(index) {
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

#[cfg(kani)]
fn any_term() -> Term {
    let choice: u8 = kani::any();
    kani::assume(choice < 5);
    match choice {
        0 => Term::Data,
        1 => Term::Context,
        2 => Term::Chezmoi,
        3 => Term::HostRead,
        _ => Term::Render,
    }
}

#[cfg(kani)]
fn any_terms() -> [Term; 4] {
    [any_term(), any_term(), any_term(), any_term()]
}

#[cfg(kani)]
const fn reads_host(term: Term) -> bool {
    matches!(term, Term::Chezmoi | Term::HostRead)
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(6)]
fn a_term_that_reads_the_host_always_refuses_the_hash() {
    let terms = any_terms();
    let length: usize = kani::any();
    kani::assume(length <= terms.len());
    let terms = &terms[..length];
    let mut host = false;
    let mut index = 0;
    while index < terms.len() {
        host |= reads_host(terms[index]);
        index += 1;
    }
    let result = verdict(terms);
    assert!((result == Verdict::HostState) == host);
    kani::cover!(result == Verdict::HostState);
    kani::cover!(result == Verdict::Admit && length > 2);
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(6)]
fn the_whole_context_is_hashed_only_through_a_rendered_partial() {
    let terms = any_terms();
    let length: usize = kani::any();
    kani::assume(length <= terms.len());
    let terms = &terms[..length];
    let mut host = false;
    let mut bare = false;
    let mut index = 0;
    while index < terms.len() {
        host |= reads_host(terms[index]);
        bare |= terms[index] == Term::Context && !(index >= 2 && terms[index - 2] == Term::Render);
        index += 1;
    }
    kani::assume(!host);
    let result = verdict(terms);
    assert!((result == Verdict::WholeContext) == bare);
    assert!((result == Verdict::Admit) == !bare);
    kani::cover!(result == Verdict::WholeContext);
    kani::cover!(result == Verdict::Admit && length == 3 && terms[2] == Term::Context);
}

#[cfg(test)]
mod tests {
    use super::*;

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
