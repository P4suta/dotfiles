# GitHub PR Evidence

Read repository and PR metadata with `gh pr view <number> --repo <owner/repo> --json number,url,headRefOid,baseRefName,headRefName,statusCheckRollup,reviewDecision`.
Use the actual repository owner, name, and PR number in the following query.
Paginate every connection; `first: 100` is a page size rather than a guarantee of completeness.

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
Fetch nested comment pages separately if a thread has more than 100 comments.
Read REST `repos/<owner>/<repo>/pulls/<number>/reviews` and `repos/<owner>/<repo>/issues/<number>/comments` with `gh api --paginate` for review commit IDs and summary status.
Compare the current head with completed review metadata and the latest bot summary; incremental review may publish completion only in the summary.
An approval, a skipped review, `Review rate limited`, a superseded run, or missing commit evidence must not be described as a fresh completed review.
Re-read the head before acting if the branch has moved.
