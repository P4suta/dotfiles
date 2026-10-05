# Windows host operations

## Defender and the Dev Drive

A Windows host can stop answering remote commands while SSH authentication still succeeds.
The session closes after about 40 seconds, the CPU idles, and only a restart recovers it.
The cause observed: a hung Microsoft Defender engine scanning short-lived state files that mutation testing wrote under `%TEMP%`.
Defender runs as a protected service, so nothing short of a restart recovers a hung engine.

To diagnose the same symptom, read the Defender Operational log for events 5008 and 3002 and the Service Control Manager log for event 7011.

The prevention: a Dev Drive, a dynamically sized virtual hard disk formatted as ReFS, with the user `TEMP` and `TMP` directories on it.
A scheduled task attaches it at startup by running `diskpart /s` with an attach script as SYSTEM.
Defender scans a Dev Drive in performance mode, so build and test scratch files no longer reach the synchronous scan path.
After a forced power-off, `Mount-DiskImage` and `Get-DiskImage` failed with access denied while `diskpart` still attached the image, so the task uses `diskpart`.

While the Dev Drive stays detached, `TEMP` has no directory and every tool that creates temporary files fails, including the agent clients with `EPERM` from `mkdir`.
Attaching it again from an elevated shell with the same `diskpart` script recovers immediately.
The storage watcher follows the temporary directory the host configures in `.chezmoitemplates/profiles/windows/dot_config/storage-scout/auto.toml.tmpl`.
