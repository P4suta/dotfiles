# Creative placements and review drafts

Read Apple's current [creative asset specifications](https://developer.apple.com/help/app-store-connect/reference/app-information/creative-assets-specifications) and [asset guidance](https://developer.apple.com/app-store/asset-best-practices/) before choosing dimensions, copy, safe areas, or formats.
For the selected placement types, query `asc asset-library specs`.
Apple's [asset management guide](https://developer.apple.com/help/app-store-connect/manage-app-information/manage-your-app-store-assets) describes placement availability and review.
Don't assume a new creative format replaces screenshots on every supported OS.

## Make each placement readable

Design the product header and search result for their actual visible areas.
A universal image can serve both placements.
Use separate images when their crops or text scale need different compositions.
Keep one clear focal idea and readable short text.
Use native screenshots when they help communicate the experience.
Keep screenshots readable at the actual preview size.
For authored styling, use `monochrome-design`.
Preserve explicit product requirements for native captures.

Use the current Apple template's actual safe area.
Inspect the downloaded template rather than inferring that the whole image remains visible.
Preserve an editable source and render an opaque export in the supported format and color space.
Check dimensions and alpha with native image tooling before uploading.
Then inspect the actual App Store Connect device previews for the header and search results.
A centered crop made locally isn't an exact substitute for those previews.

## Upload, assign, and prepare review

Track upload, placement, review, and publication as distinct states.
Discover the library ID and localization ID before writing.
CLI 6.6.1 supported the commands below during a live store preparation on November 13, 2026.
Recheck installed help and current server behavior before relying on their flags.

```sh
asc asset-library images upload --library-id "$LIBRARY_ID" --file "$IMAGE_PATH" --reference-name "$ASSET_NAME"
asc localizations placements create --localization-id "$LOCALIZATION_ID" --image-id "$IMAGE_ID" --placement-type PRODUCT_PAGE_HEADER_ASSET
asc localizations placements list --localization-id "$LOCALIZATION_ID" --placement-type PRODUCT_PAGE_HEADER_ASSET,APP_STORE_SEARCH_RESULTS_ASSET --paginate --include image
```

For search artwork, select the placement type: `APP_STORE_SEARCH_RESULTS_ASSET`.
Check that each image finished processing.
Check that each intended placement is active.
Uploading doesn't assign the artwork or submit it for review.

For an initial app version, assign artwork before adding the version to a review draft.
In the observed workflow, adding the app version embedded its assigned artwork in the version's review and locked the images.
Adding the same image as a separate item failed because an embedded review had locked it.
Inspect the image and version review states instead of retrying that duplicate item.
For standalone artwork updates to an approved app, inspect the current submission requirements.

To revise an unsubmitted draft, confirm its submission state before removing its version item to restore editability.
Keep the existing approved or selected build.
When applicable, replace an existing placement with `localizations placements swap`.
Preserve the placement group.
The CLI describes that swap relationship as a live-verified extension, so inspect the result and don't assume an automatic delete fallback.
Assign the corrected images and add the version back to the draft.
Read back both placements, embedded artwork states, selected version, and `submittedDate`.
Leave final submission to the owner when that's the requested handoff.
