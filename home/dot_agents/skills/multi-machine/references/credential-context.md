# Credential execution context

A background service can run as the owner while using a different login session from the desktop.
An agent communication error or inaccessible credential store alone proves neither expired credentials nor a locked password manager.
Check the execution context before asking the owner to log in, unlock an agent, or change authentication settings.

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

Serialize selected PowerShell metadata with `ConvertTo-Json` or `Out-String` before an immediate process exit.
Deferred display formatting can discard output at exit, so an empty display alone proves no absence.

Keep live account identifiers, machine names, key fingerprints, private paths, job records, and credentials in machine-local state.
Shared skills contain the reusable procedure only.
