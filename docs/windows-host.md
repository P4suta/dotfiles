# Windows host operations

## Defender engine hangs and the Dev Drive

A Windows host can stop answering remote commands while SSH authentication still succeeds: the session closes after about 40 seconds, the CPU is idle, and only a restart recovers it.
The cause observed was a hung Microsoft Defender engine scanning short-lived state files that mutation testing wrote under `%TEMP%`.
Defender is a protected service, so nothing short of a restart recovers a hung engine.

To diagnose the same symptom, read the Defender Operational log for events 5008 and 3002 and the Service Control Manager log for event 7011.

The prevention is a Dev Drive: a dynamically sized VHDX formatted as ReFS, attached at startup by a scheduled task that runs `diskpart /s` with an attach script as SYSTEM, with the user `TEMP` and `TMP` directories on it.
A Dev Drive is scanned in performance mode, so build and test scratch files no longer reach the synchronous scan path.
`Mount-DiskImage` and `Get-DiskImage` were refused with access denied after a forced power-off, while `diskpart` still attached the image, so the task uses `diskpart`.

When the Dev Drive is not attached, `TEMP` does not exist and every tool that creates temporary files fails, including the agent clients with `EPERM` from `mkdir`.
Attaching it again from an elevated shell with the same `diskpart` script recovers immediately.
The storage watcher follows the temporary directory the host configures (`.chezmoitemplates/profiles/windows/dot_config/storage-scout/auto.toml.tmpl`).
