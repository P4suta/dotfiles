# Local issue development

A project can prepare issues and scoped work before it has a GitHub repository.
The local provider needs Git and the installed workflow tools, and requires no GitHub executable, token, or Doppler authentication.

Start in a genuinely empty project directory.
Keep the issue body outside that directory until you register its implementation scope.

```sh
pr-workflow local init --root .
pr-workflow local create --root . --title "Preserve input" --body-file ../issue.md
pr-workflow local select --root . --issue 1 --path src
```

Initialization creates local Git metadata when needed and supports a branch without a commit.
It creates no remote repository or placeholder commit.
Issue creation applies the normal issue document checks.
Selection refuses existing dirty implementation outside open issue scopes registered before those edits.
Selection uses Git's tracked changes and nonignored untracked paths without reading generated artifact contents.
If a committed or indexed `.gitignore` is missing, selection names that file and requires its restoration before listing untracked artifacts.
Local issues can register separate or overlapping scopes.

An existing directory without a first commit uses an explicit initial import instead of normal selection.
Prepare its local issue, stage the intended artifacts, and list every imported scope.

```sh
git add --all
pr-workflow local import-preview --root . --issue 1 --path src --path Cargo.toml
pr-workflow local import --root . --issue 1 --path src --path Cargo.toml --snapshot SHA256_FROM_PREVIEW
```

The preview prints the actual staged tree, paths, and a digest without granting edit authority.
Import requires that exact digest, an unborn branch without refs or remotes, an open issue, and no unstaged or nonignored untracked artifacts.
The local provider must have no prior import or remote promotion.
Import accepts prior selections only for that same issue and branch, with every source binding still unborn.
Import replaces those selections with the reviewed baseline scopes, so include every scope needed for future edits.
Every imported artifact must fall within the explicit scopes.
Alternate indexes and index flags that hide working changes refuse.
The existing signing identity attests the baseline using trusted owner signers outside the project from the owner's global Git configuration.
Project-local allowlists supply no trust.
The evidence object has no parent and updates no branch or ref.

The signed baseline records the existing artifacts and permits future scoped edits, including repairs before the first real commit.
It doesn't claim the issue preceded those artifacts.
Changed content, scopes, branch, issue state, or local state revision invalidate an earlier preview.
Ordinary selection still refuses late adoption, and import refuses an existing source history or incompatible selections.

Supported mutation hooks require the selected open issue, actual project and branch, current source, and registered path.
Read-only discovery and issue preparation remain available.
Inspect an issue or close it explicitly.

```sh
pr-workflow local status --root .
pr-workflow local status --root . --issue 1 --state closed
pr-workflow local check --root .
mise x -- just check
```

`local check` verifies the issue prerequisite and doesn't run tests or proofs.
The project's normal check command retains its applicable lint, type, test, and formal-assurance stages.
Its shared issue gate uses the explicitly registered local provider without calling GitHub.
A closed issue, changed branch, unrelated real source, unregistered edit, or altered state without matching retained history refuses.

Issue records and history remain in owner-local storage outside the project.
Repository issue files can't grant edit authority.
Local records rely on state that the owner controls.
Their hashes identify history records without signatures.
An explicit initial import also retains a signed artifact baseline, which the provider verifies when it reads the active record.
An unborn selection binds the actual named branch and Git directory.
The first observed real commit becomes a persisted source binding under the local state lock.
A selection with a real source binding requires its tree and ancestry.

At the import boundary, a selection whose source commit object no longer exists is stale and doesn't count as a prior selection.
Import preview and import ignore these stale selections.
A dangling commit still counts as an existing source, and a previous signed import still prevents another import.
Preview leaves the local records unchanged, and a successful import replaces the stale selections while preserving their history records.

Adding a remote doesn't grant publication authority.
After publication becomes authorized, create or select an open issue in the exact remote repository and use the normal checked start.

```sh
pr-workflow start --repo OWNER/REPO --issue NUMBER --path src --promote-local
```

Promotion checks the live remote destination and issue, uses the existing supported signing identity, and links the actual retained local history.
Every dirty path must have current open local issue authority.
Remote authorization begins at that checked start and supplies no earlier remote chronology.
Normal publication authorization, signed integration evidence, required checks, and review requirements still apply.
