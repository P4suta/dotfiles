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
Never describe an approval, a skipped review, `Review rate limited`, a superseded run, or missing commit evidence as a fresh completed review.
Review completion and user-authorized merge admission are separate decisions.
Use `pull-request` for merge commands when the user explicitly chooses to merge without optional review.
Read the head again before acting when the branch moves.
