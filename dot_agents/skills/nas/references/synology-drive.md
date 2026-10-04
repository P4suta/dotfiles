# Synology Drive Transport

Locate the installed client's configured sync root instead of assuming the same path on macOS, Windows and Linux.
On-demand files can be placeholders; hydrate required inputs before archiving and verify that all intended bytes were read.
Use the client or server's current per-file status and error details after upload, not just the folder badge.
On macOS, the native File Provider's read-only item evaluation can supply `isUploaded`, `isUploading`, `isSyncPaused`, conflict and exclusion states when UI inspection is unavailable.
Record that as a provider acknowledgement rather than a server-side checksum or independent restore.
Preserve the existing task and authentication; changing or removing a sync task can affect both local and remote content.

Use current [Synology Drive Client documentation](https://kb.synology.com/en-us/DSM/help/SynologyDriveClient/synologydriveclient?version=7), [On-demand Sync guidance](https://kb.synology.com/en-us/DSM/tutorial/What_is_On-demand_Sync), and [sync failure diagnostics](https://kb.synology.com/DSM/tutorial/Why_are_files_not_synced_between_Synology_Drive_and_Drive_desktop_application) for the installed version.
