# Answer evidence

The managed UserPromptSubmit hook starts a request and supplies its session ID.
Prepare a UTF-8 draft and a JSON evidence document in a private local workspace.
Run the installed checker with those exact files.

```sh
dotfiles answer check --session SESSION --draft answer.txt --evidence evidence.json
```

The checker requires one claim for each nonempty draft line, in the same order.
Keep each sentence on its own line.
A single final file newline has no effect on the displayed-answer identity.
Every other change requires another check.
The request generation, prompt, draft, producer, retrieved source content hashes, and proof image bind the retained evidence.
An identical prompt submitted again starts a different generation.
The checker retains the exact draft, evidence document, retrieved source text, generated theorem source, and completed proof report in owner-local state.
Stop revalidates their identities, source inventory, argument inventory, controlled theorem source, and axiom and replay reports.
Missing or modified artifacts prevent completion.
Stop uses the retrieved source snapshot.
Checking an updated source requires another verification run.

## Claims

The document contains `version`, `atoms`, `claims`, and `readings`.
Unknown fields fail validation.
Each claim contains `id`, exact `text`, an optional `formula`, and a `basis` object.
IDs must be unique, and dependencies must name earlier claims.

| Basis kind | Required fields | Checked contract |
| --- | --- | --- |
| `source` | `source` | Retrieve the identified authority over HTTPS, find the excerpt, and include its URL in the draft line. |
| `user` | `quote` | Find the exact quotation in this request and the draft line. |
| `observation` | `record`, `excerpt`, `exit` | Match a retained tool observation from this request, its output excerpt, and its exit status. |
| `hypothesis` | `reason` | Use a visible hypothesis label and preserve its conditional status downstream. |
| `inference` | `argument`, `conditional` | Resolve earlier formal premises, check consistency and countermodels, and complete the proof and kernel replay. |
| `comparison` | `comparison` | Check matched roles, metric, population, period, unit, denominator, conditions, and any claimed numeric winner. |
| `parallel` | `parallel` | Check that the declared items appear and share their declared semantic role. |
| `proposal` | `reason` | Label the proposal and exclude it from factual premises. |
| `unverified` | `reason` | Label the unverified statement and exclude it from premises. |
| `question` | None | End the line with a question mark. |

The English labels are `Observation:`, `Hypothesis:`, `Under these assumptions:`, `Proposal:`, and `Unverified:`.
Use the Japanese labels for Japanese replies.
A conditional conclusion needs its visible qualification when any supporting premise is hypothetical.
Source, testimony, and observation records establish provenance contracts rather than the truth of arbitrary paraphrases.
Review the correspondence between each record, its formal interpretation, and the draft sentence.

## Formal arguments

`atoms` gives the natural-language interpretation of each Boolean variable.
A formula is a tagged expression with `op` and `args`.
`atom` takes its zero-based index.
`not` takes one formula, and `and`, `or`, and `implies` take two formulas.
An inference's `argument` contains earlier premise IDs and its `conclusion` formula.
The claim's `formula` must equal that conclusion.

```json
{
  "version": 1,
  "atoms": ["The input passes", "The output passes"],
  "claims": [
    {
      "id": "input",
      "text": "Hypothesis: The input passes.",
      "formula": {"op": "atom", "args": 0},
      "basis": {"kind": "hypothesis", "reason": "An explicit assumption for this example"}
    },
    {
      "id": "rule",
      "text": "Hypothesis: If the input passes, the output passes.",
      "formula": {"op": "implies", "args": [{"op": "atom", "args": 0}, {"op": "atom", "args": 1}]},
      "basis": {"kind": "hypothesis", "reason": "An explicit conditional assumption"}
    },
    {
      "id": "output",
      "text": "Under these assumptions: The output passes.",
      "formula": {"op": "atom", "args": 1},
      "basis": {
        "kind": "inference",
        "conditional": true,
        "argument": {"premises": ["input", "rule"], "conclusion": {"op": "atom", "args": 1}}
      }
    }
  ],
  "readings": []
}
```

Use only the finite propositional domain the checker supports.
The command rejects larger or unsupported formulas rather than approximating them.
It enumerates every assignment, rejects inconsistent premises and a premise equivalent to the conclusion, and produces a theorem from controlled syntax.
No user-supplied Lean code, axiom, placeholder, or tactic enters that theorem.
The proof assistant checks the theorem without added axioms, and a fresh kernel replays its artifact.
The truth-table preflight and kernel verification provide different checks of the declared formal problem.
Neither proves that a causal or empirical implication used as a premise is true.

The native setup command builds the repository's pinned proof image and records its immutable ID.
Run it from the validated dotfiles source.

```sh
mise x -- just install-answer-prover
```

Missing images, a different release, missing axiom reports, incomplete replay, or a bounded-run stop prevent proof evidence.
Proofs run without network access and under the installed resource policy.

## Sources

A source contains `url`, `authority`, `title`, `publisher`, `locator`, and `excerpt`.
Its authority is `public`, `dictionary`, or `technical`.
The checker registers known issuing bodies and dictionary providers for those classes.
It refuses credentials, custom ports, non-HTTPS transport, misleading host suffixes, and a host in the wrong class.
It retrieves bounded textual content and requires the exact excerpt.
The claim needs a supported textual record that the checker can retrieve.
Don't relabel a host as a different authority class to evade a refusal.

The source's domain and an excerpt are necessary provenance checks.
They don't certify that the cited passage entails the paraphrase or that a dictionary record matches the intended sense.
The title, publisher, locator, authorship, context, and semantic relationship still need inspection.
Don't interpret a successful retrieval as a total credibility score.

## Readings

A reading contains `written`, `kana`, `romanized`, `context`, `source`, and `standard`.
The lexical source must be public or a major dictionary entry.
Its excerpt must contain the original term and the declared kana together.
Katakana and hiragana normalize for this match.
The draft must contain the original term and the spelling it discusses.

`{"name":"cabinet2025-kana"}` selects the supported Cabinet table with the lexical kana vowels retained.
The converter handles the supported Japanese morae, moraic nasal boundaries, sokuon, and explicit long-vowel marks.
It rejects unsupported morae and running-text cases that require more context.
It doesn't choose a reading, parse particles, decide word boundaries, or generate conventional proper-name spellings.

`{"name":"attested","title":"Standard title","source":{...}}` requires a second identified record that contains the original term and its established spelling together.
Use this for an applicable standard's terminology or an established spelling outside the converter's supported conventions.
Keep the lexical reading evidence and spelling evidence separate.

## Comparisons and parallel relations

A comparison has `first`, `second`, `preference`, and an optional `winner`.
Each alternative has `label`, earlier evidence claim `basis`, `axis`, and an optional signed integer `value`.
Each axis has `role`, `metric`, `population`, `period`, `unit`, `denominator`, and `conditions`.
Every axis field must match between the alternatives.
Normalize decimal measurements into an explicitly stated integer unit before using the numeric comparison.
`preference` is `higher` or `lower`.
A missing measurement or a tie can't support a unique numeric winner.
This establishes a ranking for the declared criterion.
Different objectives need an explicit decision rule.

A parallel relation has a common `role` and `items` containing `text` and `role`.
Supported roles are `objective`, `principle`, `method`, `mechanism`, `implementation`, `observation`, `outcome`, and `limitation`.
The checker rejects a declared role mismatch, duplicated items, and items absent from the draft.
Semantic misclassification remains a separate review obligation.
Keep a theory-to-case analysis explicitly asymmetric instead of forcing it into the peer-comparison schema.

## Tool observations

Capture an already authorized read or project check with the observation command.
Run it within the project's pinned environment when applicable.

```sh
mise x -- dotfiles answer observe --session SESSION git -- status --short
```

The wrapper permits selected Git and GitHub reads and named project check commands.
It records the actual arguments, directory, exit status, and output under this request.
Use the returned record ID, a real output excerpt, and the actual status in the claim.
A failed command remains a failed observation and can't become a successful test result by changing the claim.
The wrapper grants no authority to run another operation, spend a review budget, or start a long campaign.

## Client boundary

The final Stop check compares the displayed reply with this request's retained evidence.
Missing, changed, or stale evidence requests a correction.
The bounded continuation reports incomplete work if the evidence still fails the gate.
Stop runs after display and can't retract the initial text.
Hook installation also requires the client's supported trust and reload procedure.
Report a client without an effective hook as uncovered.

The checker protects the declared contract in a cooperative owner-controlled environment.
An agent with the same filesystem privileges can change local configuration or records.
It also can't decide whether an arbitrarily classified sentence hides a different semantic claim.
Don't promise adversarial isolation or universal natural-language correctness from this mechanism.
