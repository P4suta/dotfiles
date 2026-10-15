# Spoken development notifications

Dotfiles uses eight prepared Japanese recordings for commit and push start, failure, and completion, agent turn completion, and requests for input.
Runtime notifications don't synthesize speech or contact a service.
The runtime plays only fixed recordings.
It never reads command arguments, project paths, credentials, or conversation text aloud.

Dotfiles manages frequency `all` by default.
Choose a machine-local override with `dotfiles notifications frequency off`, `attention`, `results`, or `all`.
Profile runs preserve the override.
`dotfiles notifications status` reports the configuration, recording availability, and native player without playing sound.
`dotfiles notifications test push-completed` intentionally plays one recording on a supported local desktop.

The Git wrapper observes the actual commit and push result, including a refusal before delegation.
Nested internal Git operations, CI, SSH, and headless execution stay quiet.
Notification failures preserve the operation's exit status and streams.
They can't grant approval or bypass a gate.

Client support follows the events the client actually exposes.
Codex uses its supported turn-completion notification.
This integration doesn't observe input waiting.
Claude Code uses supported Notification events for permission, elicitation, and idle reminders.
Its Stop hook doesn't establish successful completion because another hook can hold the turn.
User prompt and tool result callbacks cancel a waiting reminder for that session.
OpenCode requires an observed busy session and a completed assistant message for the current user request before an idle event can announce completion.
Cancellation, errors, unfinished tools, stale messages, and unobserved sessions remain quiet.
Answered permission and question requests cancel their queued input reminder.

The desktop queue holds at most eight requests and uses one native player at a time.
Duplicate or cancelled requests and queued requests older than thirty seconds don't play.
The notification loader checks each recording with the fixed manifest.
Each file must use mono, 22,050 Hz, sixteen-bit pulse-code modulation audio and last at most five seconds.
The native player has an eight-second deadline.
Recording availability and a successful player result don't establish that anyone heard the sound.

Maintainers can intentionally regenerate the recordings with `dotfiles notifications prepare --output PATH` on Windows with an installed Japanese voice.
Preparation writes the fixed waveform audio files and their manifest without playing them.
Supported desktops share the checked recordings, so other hosts don't need speech synthesis.
