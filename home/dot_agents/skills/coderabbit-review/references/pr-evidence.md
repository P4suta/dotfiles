# GitHub PR evidence

Read repository and PR metadata with `gh pr view <number> --repo <owner/repo> --json number,url,headRefOid,baseRefName,headRefName,statusCheckRollup,reviewDecision`.
Paginate every connection, because `first: 100` sets only a page size.

```graphql
query($owner: String!, $name: String!, $number: Int!, $cursor: String) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      headRefOid
      reviewThreads(first: 100, after: $cursor) {
        pageInfo { hasNextPage endCursor }
        nodes {
          isResolved
          isOutdated
          path
          line
          comments(first: 100) {
            pageInfo { hasNextPage endCursor }
            nodes {
              author { login }
              body
              url
              commit { oid }
              originalCommit { oid }
            }
          }
        }
      }
    }
  }
}
```

Match the installed CodeRabbit bot identity, normally `coderabbitai` in GraphQL and `coderabbitai[bot]` in REST.
Fetch nested comment pages for a thread with more than 100 comments.
Read REST `repos/<owner>/<repo>/pulls/<number>/reviews` and `repos/<owner>/<repo>/issues/<number>/comments` with `gh api --paginate` for review commit IDs and summary status.
Compare the current head with completed review metadata and the newest bot summary, because an incremental review may report completion only in the summary.
An incremental review covers updates since the preceding run and need not repeat the full PR range or the preceding run's selected files.
Fetch [commit statuses](https://docs.github.com/en/rest/commits/statuses) and [Check Runs](https://docs.github.com/en/rest/checks/runs#list-check-runs-for-a-git-reference) for the exact head.
Check the CodeRabbit app identity and completed successful review output instead of relying on the check's name or conclusion alone.
Accept an empty current-head `APPROVED` review after verifying its submission time and successful completion.
An approval alone, a skipped review, `Review rate limited`, a superseded run, or missing commit evidence establishes no fresh completed review.
Review completion and user-authorized merge admission are separate decisions.
Use `pull-request` for merge commands when the user explicitly chooses to merge without optional review, including the standing current-head PR quota exception.
Read the head again before acting when the branch moves.

## Rate-limited optional reviews

Use `pull-request` for the standing quota exception when an authorized personal non-fork PR merge hits a current-head review limit.
Route to neighboring skills by name when a scoped installation can omit their files.
The provider's [rate-limit comment](https://docs.coderabbit.ai/management/rate-limits#when-a-review-is-rate-limited) establishes missing analysis even when its check passes.
Read both completion APIs and bind the actual notice to the inspected head before merging.
Other unavailable states receive no standing merge authorization.

## Check Runs and commit statuses

On 2026-11-11, CodeRabbit published a completed successful Check Run for the reviewed head while the commit-status API returned an empty status array.
The installed merge helper read only commit statuses and rejected the completed review.
Distinguish a helper's unsupported representation from missing service analysis.
Repair the helper or use the user's explicitly authorized merge route without manufacturing a legacy status or changing repository protections.

## Updating a reviewed PR

If the installed `push.ready` guard refuses an update, follow its Draft cycle with `gh pr ready NUMBER --repo OWNER/REPO --undo`.
Push the verified update, wait for its required checks, and use `pr-workflow ready` to start its automatic review.
Returning an already authorized PR to review after its scoped fixes uses the same authorization.
