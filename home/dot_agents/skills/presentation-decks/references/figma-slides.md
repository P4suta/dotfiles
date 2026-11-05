# Editing a deck in the slide editor

Read the deck through the Plugin API, because the design-file metadata reader rejects Slides files.
Read the grid rows from the slide grid node, and rename a section through its row node.
Apply a new grid in one script and read it back in a separate script, because a read in the same script can return the earlier order.
Pass every slide to the grid setter, because the setter refuses a grid that omits one.
Clone a designed slide to reuse its layout, then replace its header with a header from the destination section.
Mark slides kept out of the talk as skipped, and leave them in a named section.
Hold the page counter in a header component text property, and update every counter in one pass after a reorder.
Give header tabs, appendix references, and table-of-contents entries a node hyperlink to the slide they name.
Write the script to each slide's speaker notes.
Load each text node's current fonts before changing its characters.
Keep the return value of one script under the tool's size limit by reading slides in batches.
