---
status: amended
date: 2026-10-13
changed_by: [report-optional-notification-failures-with-fixed-warnings.md]
---
# Speak fixed development events through an optional desktop queue

## Context

Short beeps do not identify which operation needs the owner's attention.
The owner wants Japanese speech for commit and push start, failure, and completion, and supported agent completion and input requests.
Notification must preserve development gates and avoid exposing private command or conversation data.

## Decision

Use eight prepared Japanese WAV recordings with a checked manifest.
The runtime accepts only the closed event set and opaque request identities.
The pure notification policy decides whether to queue a request from frequency, desktop availability, origin, age, duplicate state, cancellation, and queue capacity.
The platform adapter owns the bounded machine-local queue and serial worker.
The dotfiles adapter validates recording bytes and invokes the native desktop player with a deadline.

The managed default is `all`, matching the owner's requested start, failure, and completion frequency.
A separate unmanaged owner override survives profile application.
CI, SSH, internal nested operations, and unsupported desktops stay quiet.
Optional notification failures cannot change an operation's result or streams.
The fixed diagnostic warnings in [the later decision](report-optional-notification-failures-with-fixed-warnings.md) partly supersede this stream guarantee.

Git notifications follow the wrapper's observed lifecycle.
Agent adapters announce only supported and observed events.
An agent Stop hook alone does not establish completion because another hook can refuse it.
OpenCode idle alone does not establish completion because cancellation can also produce idle.
Foreign notification handlers remain registered and retain their original arguments.

Recording synthesis is an explicit offline preparation operation.
Playback performs no synthesis, network request, or arbitrary text rendering.
The same checked recordings are deployed to every supported profile.
Availability, a playback result, and human confirmation remain distinct evidence.

## Alternatives

Keeping short beeps would leave the operation unidentified.
Runtime synthesis would add synthesis work to playback and replace the contract that playback uses checked recordings.
Treating an agent Stop hook or OpenCode idle as completion would conflate completed work with possible refusals or cancellation.

## Consequences

The owner can identify important operations without watching each tool's interface.
Unsupported client events remain unavailable until the client provides a dependable observation boundary.
The bounded queue suppresses repeated and stale reminders without blocking the development operation.
Missing recordings or a player leave notifications unavailable while preserving ordinary development.
The command reference describes frequency selection, explicit playback tests, preparation, and supported client events.
