# Exact optional properties

The OpenCode AI declaration patch admits explicit `undefined` values already produced by the SDK's schema types.
It preserves dependency declaration checking and `exactOptionalPropertyTypes`.
The patch changes no runtime code and applies only to `@opencode/ai@2.2.5`.
Remove it when the pinned SDK declarations admit their own schema outputs without modification.
