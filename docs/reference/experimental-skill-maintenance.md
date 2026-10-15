# Experimental skill maintenance

Skill observations, automated maintenance reviews, and completion holds default to off.
The skill instructions and required repository verification remain available.
Read the relevant skill files directly during ordinary development.

The mode belongs to each machine's home directory.
Chezmoi manages neither mode marker as a target.
Applying dotfiles preserves both the local choice and existing maintenance history.

## Enable an experiment

Create `~/.config/skill-ops/experimental` as a regular file containing exactly `enabled` followed by one LF byte.
Remove `~/.config/skill-ops/disabled` only when intentionally ending a previous explicit stop.
Keep the parent directories as real directories.
A symlink, extra whitespace, CRLF, invalid content, missing file, or lookup failure leaves the mode off.

Preview and apply the affected client settings and instructions through their native chezmoi profile, then use the repository's scoped adapter installation.
Use the client's normal hook trust and session refresh process.
Registration, activation, collection, and assessment remain distinct outcomes.
Enabling the experimental mode authorizes no external review or unrelated operation.

## Stop an experiment

Remove the experimental opt-in or create the local `~/.config/skill-ops/disabled` marker.
The stop marker overrides a valid opt-in.
The runtime returns before collecting observations or assessing maintenance.
Cached calls to owned hooks still check the mode, so a registered Stop handler can't hold completion while the mode stays off.
Apply the affected settings or run scoped adapter installation to remove those owned handler registrations.
Unrelated handlers, repository verification hooks, prose checks, and process timing remain active.

## Interpret the result

With the mode off, `skill-ops` reports no assessment.
Its Stop hook returns `{}`.
The off mode provides no review evidence.
It closes no historical finding.
Existing observations, incidents, reports, and dispositions remain on their original machine.
Experimental operation retains the engine's existing evidence-integrity and pending-maintenance rules.

The [decision record](../adr/experimental-skill-maintenance-by-explicit-opt-in.md) describes the default, trust boundaries, and preserved contracts.
