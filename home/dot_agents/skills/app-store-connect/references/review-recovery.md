# Prepare a rejected submission for manual resubmission

Read Apple's [unresolved submission procedure](https://developer.apple.com/help/app-store-connect/manage-submissions-to-app-review/manage-a-submission-with-unresolved-issues) before changing an existing rejected submission.
Apple separates editing rejected items from resubmitting them.
A submission with unresolved issues can't receive more items.
Removing an item prevents adding it back to the same submission.
Preserve the existing app version item when correcting it.

Inspect the submission, item, app version, selected build, rejection details, and review information before writing.
Complete the required corrections and the candidate's verification first.
For a binary change, select the processed replacement build and update any build-specific recording or review notes.
Check attachment processing and compare the saved notes with the intended content.
Preserve the review contact and unrelated accepted items.

CLI 6.6.1 exposed an item resolution operation during a live rejected-submission preparation on November 14, 2026.
Recheck installed help and current API behavior before using it:

```sh
asc review items update --id "$REVIEW_ITEM_ID" --resolved true
```

Read the item back and confirm its version relationship and resolved state.
Confirm the selected build, required metadata, attachments, and release option again.
A historical submission can keep a non-null submission timestamp while awaiting resubmission.
Compare that timestamp with the value read before the correction and inspect the current state.
Don't remove the item or cancel review history to force a null timestamp.

Check all submission items for unresolved rejections before the handoff.
Correct remaining items within the authorized scope and report any blockers outside that scope.
Describe the submission as ready only after resolving every rejected item.
When the requested handoff leaves the final button to the user, stop with the corrected item ready for the separate resubmission action.
Don't invoke submission wrappers or send an App Review message without authorization for that operation.
The [fresh-submission procedure](https://developer.apple.com/help/app-store-connect/manage-submissions-to-app-review/submit-an-app) also distinguishes adding an item from submitting it.
