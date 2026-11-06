# Security policy

## Supported versions

There is no release yet. Only the current `main` branch receives fixes.

## Reporting a vulnerability

Report a vulnerability privately: open the repository's **Security** tab and choose **Report a vulnerability**. Do not
open a public issue or pull request for it.

Include what you found, how to reproduce it (a packet capture, world file or command string if there is one), the
commit you tested, and what an attacker gains. A report is acknowledged on a best-effort basis; this is a volunteer
project and has no response-time guarantee.

## What counts

This server is a port of the vanilla Minecraft Java Edition 26.3 server, so the vanilla behaviour is the baseline.
Reports about the following are in scope:

- a remote client that crashes, hangs or exhausts the memory of the server through the wire protocol, the login and
  encryption path, command strings or the JSON-RPC management API;
- a crafted world, region, NBT or data pack file that panics the server or reads outside the data it should;
- a way around authentication, the whitelist, operator permissions or the EULA check.

Behaviour that vanilla has too (for example a known vanilla exploit) is not a vulnerability of this port; report it as a
parity question in a normal issue only if this server differs from vanilla.

## Known advisory

`cargo deny check advisories` reports RUSTSEC-2023-0071 (`rsa`, a timing side channel in private-key operations). There
is no patched release. Vanilla's login handshake requires an RSA-1024 key exchange, and the private-key decryption of
the challenge and shared secret can be reached by an unauthenticated client in online mode. The decryption is blinded and
every failure gets the same answer, but no mitigation is claimed. The advisory is ignored in `deny.toml` with this
reason; the other uses of `rsa` involve only public keys.
