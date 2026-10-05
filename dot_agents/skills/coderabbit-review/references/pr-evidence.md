# Pull request evidence

Read repository and PR metadata with `gh pr view <number> --repo <owner/repo> --json number,url,headRefOid,baseRefName,headRefName,statusCheckRollup,reviewDecision`.
Fill the actual owner, name, and PR number into this query.
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

Match the installed CodeRabbit bot identity, normally `coderabbitai` in GraphQL and `coderabbitai[bot]` in the Representational State Transfer (REST) API.
Fetch nested comment pages for a thread with more than 100 comments.
Read `repos/<owner>/<repo>/pulls/<number>/reviews` and `repos/<owner>/<repo>/issues/<number>/comments` with `gh api --paginate` for review commit IDs and summary status.
Compare the current head with completed review metadata and the newest bot summary, because an incremental review may report completion only in the summary.
Never describe an approval, a skipped review, `Review rate limited`, a superseded run, or missing commit evidence as a fresh completed review.
Read the head again before acting when the branch moves.
