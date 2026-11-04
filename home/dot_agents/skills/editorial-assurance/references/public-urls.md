# Stable public article URLs

Choose a public URL from the resource's durable identity before publication.
Keep the URL independent of presentation, storage paths, editorial status, and a headline that may change.
W3C's [URI persistence guidance](https://www.w3.org/Provider/Style/URI) explains why moving files or changing publishing software need not change public identifiers.

Use descriptive words and hyphens between words, following [Google Search's URL guidance](https://developers.google.com/search/docs/crawling-indexing/url-structure).
Include the subject and the scope needed to distinguish the resource from neighboring publications.
A release report needs its canonical product or implementation, covered releases, any channel or associated platform that changes the scope, and the report's purpose.
Take product names and release identities from the applicable primary records.
Don't derive identity from a translated headline or reuse an unrelated naming example as a universal template.

Choose the shortest slug that preserves the complete identity.
An arbitrary length target, repeated keywords, random suffix, or publication date added only for uniqueness provides no semantic distinction.
Add a date when the date identifies the resource, such as a daily report.
Different document purposes or scopes need distinct identities even when they cover the same product release.
Check the actual destination for collisions with both posts and static pages before reserving a slug.

The `ghost-cms` publisher profile requires lowercase ASCII letters and digits with single internal hyphens, at least one letter, and at most 64 bytes.
The profile is a selected publishing contract, rather than the complete URI grammar.
[RFC 3986 section 2.3](https://www.rfc-editor.org/rfc/rfc3986#section-2.3) also permits uppercase letters and other unreserved punctuation.
Apply another publisher's documented route constraints when the destination differs.

Declare a supported article identity independently of its title in front matter.
For release analysis, use the publisher's checked identity schema and derive the slug before authoring cross-article links.

```json
{"kind":"release-analysis","subject":"language","releases":[{"version":"3.2"}]}
```

```sh
ghost-cms slug derive article-identity.json
ghost-cms slug check articles/
```

Store the identity as `url_identity` beside the derived `slug` in each source article.
The common preparation gate checks that a declared identity matches its slug during validation, preview, drafting, and publication.
The strict `slug check` command additionally refuses articles without an identity declaration.
Legacy sources without a declaration remain accepted by ordinary preparation.
Use strict checking for a collection that adopts the identity policy.
The checks establish syntax, declared-scope agreement, and collection uniqueness.
They don't establish that the declared release exists, that the report covers its stated scope, or that an unqueried destination has no collision.
Compare those claims with the article and primary release records.

Treat an already published URL as persistent.
Changing a published slug requires an explicit migration with redirects, updated internal links, and destination readback.
Don't rename a public article to follow a later aesthetic preference.
Use stable Ghost post IDs for metadata amendments so a completed rename doesn't lose the target.
Keep authenticated editor URLs and local preview paths separate from public article URLs.
Search-engine route requirements don't govern an authenticated editor's app routing.

After a draft rename, regenerate previews and validation records from the changed source.
Update source links and preserve a useful redirect for an existing local review bookmark.
Read back the stored slug, publication state, tag associations, editor content, and public route after applying authorized changes.
