# Credential execution context

A background service can run as the user while using a different login session from the desktop.
An agent communication error or inaccessible credential store alone proves neither expired credentials nor a locked password manager.
Check the execution context before asking the user to log in, unlock an agent, or change authentication settings.

Compare the exact executable and version, Git's configured SSH and signing programs, the user, relevant environment variables, and the login session.
Inspect only the metadata needed for that comparison.
Avoid dumping the full environment, credential files, tokens, or private keys.
Use the same bounded command in the existing authorized desktop context to distinguish session access from an account failure.
Keep the normal agent confirmation and signing policy.
Report the observed failure precisely when a working comparison is unavailable.

On Windows, the configured native OpenSSH client may reach the 1Password agent while a bundled Git client can't.
Check the configured executable and agent pipe before replacing the client or changing services.
The Windows OpenSSH agent service can intentionally remain off when 1Password provides the agent.
A service session and an interactive desktop session can have different access to the agent pipe and OS credential manager, even for the same user.
Check the macOS command's login session and keychain access.
On Linux, compare the configured agent socket and its access from the command's session.

Prefer the maintained runner for the existing desktop session when the authorized command needs that session's credentials.
When using a temporary OS task, bind it to the exact executable, arguments, working directory, and existing logged-in user.
Use the user's limited privilege level, a bounded runtime, and a unique task identity.
Preserve the command's exit status, distinguish an unfinished task from success, and remove only that task after completion.
This diagnostic route grants no permission for a new remote session, elevation, credential changes, or further external actions.
Keep signing receipts and credentials on the machine that produced them.

When a CLI reports that secure credential storage is unavailable, diagnose session access before repeating login.
Verify which credential store the CLI uses after any authorized repair.
Treat plaintext fallback storage as unresolved until you verify secure storage or remove the temporary credential through the CLI's normal logout.

For Windows error 448, inspect the executable selected by the launcher, its reparse attributes, and its target before changing authentication or security settings.
Run the same bounded command in the same environment with the selected proxy and a regular copy of its target, then compare their results before repairing the cache.
On 2026-11-14, a native-check cache reproduced that error with symbolic Cargo proxies to `rustup.exe`, while byte-identical regular copies passed in the same isolated environment.
For a confirmed cache repair, check active users of the cache, record the target hash, and preserve displaced entries before replacing only the affected proxies.
Compare each installed copy with the recorded target hash, verify regular-file attributes, and rerun the failed command in its original environment before resuming project checks.

Serialize selected PowerShell metadata with `ConvertTo-Json` or `Out-String` before an immediate process exit.
Deferred display formatting can discard output at exit, so an empty display alone proves no absence.

Keep live account identifiers, machine names, key fingerprints, private paths, job records, and credentials in machine-local state.
Shared skills contain the reusable procedure only.
