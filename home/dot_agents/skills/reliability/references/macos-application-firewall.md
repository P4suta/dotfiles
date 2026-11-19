# macOS firewall observations

The macOS firewall filters inbound traffic for each app.
`/usr/libexec/ApplicationFirewall/socketfilterfw --listapps` lists each recorded binary with its firewall decision.
The same command reads the global state with `--getglobalstate`, `--getblockall`, `--getallowsigned`, and `--getstealthmode` without administrator rights.
Cargo development binaries carry an ad-hoc linker signature, and `codesign -dv` shows it.

## Comparison of 2026-11-16 on macOS 27.2

The firewall was on, with block-all and stealth mode off and both signed-software allowances on.
A binary listed with `(Block incoming connections)` received the reply to its own unicast request.
The same binary received no reply to limited or directed broadcast requests to the same device.
A second binary listed with `(Allow incoming connections)` received the broadcast replies from the same device minutes apart through the same search code.
The Apple-signed `/usr/bin/nc -u` received the same unicast reply, which showed that the device answered.
A single-port TCP connect sweep with `nc -z -G` located the device, which didn't answer a broadcast ping.
These runs didn't test whether a binary's firewall decision survives a rebuild of an ad-hoc signed binary.
Treat the comparison as a dated observation, and repeat it before a decision depends on it.
