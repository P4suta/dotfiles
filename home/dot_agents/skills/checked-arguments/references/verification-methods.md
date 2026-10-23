# Verification methods

Choose a method that covers the actual claim and its semantics.
The entries below complement each other and don't form a ranking of universally stronger tools.
The [informal logic literature](https://plato.stanford.edu/entries/logic-informal/) treats relevance, argument evaluation, and context as substantive concerns beyond formal derivability.

| Claim or failure class | Suitable checks | Evidence and limits |
| --- | --- | --- |
| Propositional entailment | Complete truth tables, SAT countermodel search, checked natural deduction, proof-assistant kernels | Declare every atom and cover the full admitted domain or check a complete proof. |
| Quantified claims | First-order logic, SMT within a supported theory, proof assistants | Preserve quantifier order, domains, witnesses, and side conditions. |
| Arithmetic and algebra | Exact arithmetic, dimensional analysis, SMT for the actual theory, theorem provers | Model rounding, overflow, partial operations, and units explicitly. |
| Program behavior | Language-native verification, contracts, abstract interpretation, property checks | Import production behavior and expose unsupported language features. |
| State transitions | Invariant proofs, induction, temporal logic, model checking | State initial conditions, transitions, fairness, safety, and liveness assumptions. |
| Finite alternatives | Exhaustive enumeration, decision diagrams, optimization certificates | Prove completeness of the enumerated candidate set and preserve constraints. |
| Probability | Normalization, Bayes calculations, likelihood checks, calibration, sensitivity analysis | State priors, likelihoods, dependence assumptions, and the reference class. |
| Statistical estimates | Design review, reproducible analysis, intervals, null comparisons, robustness checks | A statistical procedure doesn't establish its sampling or causal assumptions. |
| Causal effects | Counterfactual models, causal graphs, identification arguments, design checks | Identification and estimation each need justification. |
| Explanatory hypotheses | Competing hypotheses, discriminating predictions, negative controls, independent observations | A leading explanation can be wrong or omit an explanation with stronger support. |
| Analogies | Explicit relation mapping, relevant similarities, disanalogies, transfer tests | Superficial similarity supplies no general transfer theorem. |
| Definitions and classifications | Type distinctions, partition checks, examples, boundary cases, counterexamples | Consistent definitions need independent adequacy for the intended subject. |
| Comparisons and parallel claims | Shared axes, matched scope, units, denominators, abstraction, evidence quality | Matching declared metadata doesn't prove its correspondence with the descriptions. |
| Recommendations | Feasibility checks, explicit objectives, Pareto analysis, sensitivity, reversibility | Keep evidence about consequences distinct from values and priorities. |
| Testimony and personal experience | Accurate attribution, contextual clarification, source preservation | Accepting a report as testimony doesn't establish its proposed cause or population frequency. |
| Natural-language coherence | Claim maps, definition tracking, critical questions, counterinterpretation | Semantic review remains a judgment unless a supported formal representation covers it. |

## Independent verification

Vary both the implementation and the representation when useful.
A proof assistant and an independently written complete evaluator can expose translation and implementation disagreements.
A proof-producing SAT or SMT solver with a separate certificate checker can reduce trust in a solver's bare success report.
Two frontends that share the same unchecked assumption or solver don't provide independent coverage of that component.
Identify shared parsers, libraries, models, and premises when reporting independence.

Use proof kernels to check proof terms rather than accepting a model's account of a proof.
Inspect theorem statements, dependencies, imported axioms, and hidden assumptions.
For Lean, [the axioms documentation](https://lean-lang.org/theorem_proving_in_lean4/Axioms-and-Computation/) explains the role of axioms and computation.
Reject placeholders, admitted goals, added axioms that assert the result, and native-evaluation shortcuts outside the accepted trust model.
Independent kernel replay checks the saved artifact rather than trusting a previous process's report.

Search for countermodels before and after formalization.
Before formalization, challenge the interpretation and missing premises.
After formalization, challenge the formal implication in the actual admitted domain.
Reconcile disagreements rather than choosing the checker that agrees with the preferred conclusion.

## Scope and failures

Name the input domain, bound, assumptions, theorem or property, production or model link, tool identity, and exact result.
Distinguish verified, refuted, inconclusive, unsupported, unrun, and stale results.
Keep a countermodel and explain its effect on the claim.
A timeout supplies no proof and doesn't by itself refute the claim.
A failed parser or missing tool supplies no logical verdict.
An empty inventory supplies no assurance.

Keep an unsupported conclusion unresolved until a suitable checker covers it.
Disclose a change in the claim's scope.
Preserve difficult cases and the connection between the requested result and its formal statement.
Use bounded project runners on owner machines and agree on a resource window before an hours-long campaign.
More computation can't repair an unexamined premise or a mistranslated theorem.
