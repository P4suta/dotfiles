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
Local issues can register separate or overlapping scopes.

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
Their hashes identify records without signatures.
An unborn selection binds the actual named branch and Git directory.
The first observed real commit becomes a persisted source binding under the local state lock.
A selection with a real source binding requires its tree and ancestry.

Adding a remote doesn't grant publication authority.
After publication becomes authorized, create or select an open issue in the exact remote repository and use the normal checked start.

```sh
pr-workflow start --repo OWNER/REPO --issue NUMBER --path src --promote-local
```

Promotion checks the live remote destination and issue, uses the existing supported signing identity, and links the actual retained local history.
Every dirty path must have current open local issue authority.
Remote authorization begins at that checked start and supplies no earlier remote chronology.
Normal publication authorization, signed integration evidence, required checks, and review requirements still apply.
