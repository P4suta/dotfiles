# Image attachments and historical figures

[GitHub CLI attachments](https://docs.github.com/en/github-cli/github-cli/attaching-files-with-github-cli) require repository push access.
Pass the same files with `--attach` to rewrite local image references in a body.
Resolve relative files from the body file's directory, preserve alternative text and image titles, and verify the published body.
Keep D2 source in a [collapsed code section](https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/organizing-information-with-collapsed-sections).
Media attachment support doesn't make a D2 source file an image.

The [PR creation command](https://cli.github.com/manual/gh_pr_create) can create a PR with successful attachments and exit unsuccessfully when another upload fails.
Preserve the printed publication URL and inspect the existing document before retrying.
An unsuccessful process doesn't establish that GitHub created nothing.

Use `pr-workflow migrate-figures --owner OWNER` to inventory historical figure branches without publishing changes.
Use `--apply` after reviewing the inventory to preserve branch history, image bytes, and D2 source and migrate document references.
Repeat the checked migration to recover completed uploads and continue unfinished work.
Keep legacy UTF-8 text files in fenced source sections and preserve their filenames.
Send only supported media through `--attach`.
Use `--apply --retire` after deploying attachment publication to all writing machines.
Retirement requires complete pagination and readback, preserved source and image content, no remaining managed document references, and unchanged branch identities.

Inventory issue and PR bodies, issue comments, inline review comments, review summaries, and release descriptions, including references from other owned repositories.
The [review API](https://docs.github.com/en/rest/pulls/reviews) returns `html_url` and `pull_request_url`, so construct a review's API endpoint from its repository, PR number, and review ID.
Preserve concurrent edits by rereading each document before replacement and verifying the resulting body.
GitHub body updates provide no transaction across every document and branch, so a scan establishes the observed state and assumes no unobserved new legacy references during retirement.

Use an explicit expected object ID with [Git's lease](https://git-scm.com/docs/git-push) when deleting a figure branch.
The lease refuses a branch that moved after inspection and doesn't rely on mutable remote-tracking refs.
Keep retirement inside the checked workflow so that a repository-local permit binds each leased deletion to its archived ref and object identity.
Keep ordinary force and deletion refusals and never substitute a force waiver or a stack lease.
Keep the verified bundle and migration manifest until their retention policy permits archival cleanup.
Install the Git wrapper and every policy companion from the same producing source, then exercise their actual dispatch path after scoped rollout.
A unit fixture with a matching companion doesn't verify a native installation that retained an earlier companion.

Scoped authoring rollout must preserve unrelated installed skill files and existing review records.
A recorded installation identity doesn't grant review credit to a changed skill.
Keep experimental maintenance off unless the machine has explicitly opted in.
