# Security

## Supported versions

Only the latest release gets fixes.

## Reporting

Report anything security related to the owner directly, not in a public issue.

## What the app touches

- **Local game files.** `gameinfo.gi` in the Steam library, and `cfg/video.txt` beside it. Only the
  keys the user chose are written, and a one-time `.dlfovfixer.bak` copy of each file is made before
  the first change to it.
- **Its own settings.** `%APPDATA%\DL-FOV-Fixer\config.json`, readable by the signed-in user only,
  because that is what the folder already is.
- **The Windows Run key.** One value named `DL-FOV-Fixer` under
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, written only while the startup setting is on.
  Nothing else in that key is read or changed.
- **The network, for updates only.** An HTTPS request to this repository's public GitHub Releases. No
  account, no telemetry, nothing is uploaded.

The app is not related to anti-cheat and does not touch the game while it runs. A locked file is
waited out, not forced.

## Untrusted input

- **A pasted config.** Treated as text: keys are validated against a shape, values are taken as
  single tokens, and a key that would collide with a sub-block is skipped rather than written.
- **`config.json`.** Unknown keys are ignored and a corrupt file falls back to defaults, so a bad file
  cannot stop the app from starting.
- **A downloaded release.** The app reads `manifest.json` only after `manifest.sig` verifies against
  the public key built into it, and runs an installer only when its size and SHA-256 match that
  signed manifest ([ADR 0006](docs/adr/0006-trust-updates-through-a-signed-manifest-not-a-certificate.md)).
  Download paths are limited to this repository's release assets, and every read has a size limit.

## Signing material

The update key's private half lives only on the owner's offline drive and in the
`DLFOVFIXER_MANIFEST_SIGNING_KEY` Actions secret. Only the public key is in the repository
([contracts/keys/README.md](contracts/keys/README.md)). A leaked key is rotated: a new key, and one
manual update for every user.

Certificates, passwords and `.pfx` files never enter the repository. `tools/setup-windows-signing.ps1`
enrols a certificate into the current user's store or a CI secret, and
`tools/sign-windows-artifacts.ps1` reads it from the environment, uses it and discards it, so no
caller ever holds it.

## Recovery

- **A bad apply.** Verify the game files in Steam, which restores a clean `gameinfo.gi`. Copy
  `gameinfo.gi.dlfovfixer.bak` back only if the game has not been updated since that backup was
  made, because it holds that day's version of the file.
- **A bad release.** Delete that GitHub Release so no installed copy is offered it, then publish a
  fixed version.
- **A leaked signing certificate.** Revoke it with the issuer, enrol a new one, and publish the next
  release signed with it.
