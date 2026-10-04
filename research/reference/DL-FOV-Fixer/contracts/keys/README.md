# Keys

`release-manifest-public.b64` is the update channel's public key: ECDSA P-256, as base64 DER
SubjectPublicKeyInfo. The app builds it in and installs an update only when the release's
`manifest.json` is signed with the matching private key (ADR 0006).

The private key is not in this repository. It lives on the owner's offline flash drive in
`DL-FOV-Fixer-signing\release-manifest-signing.pem`, with a README.txt beside it, and in the
`DLFOVFIXER_MANIFEST_SIGNING_KEY` GitHub Actions secret that `release-windows.yml` signs with.
`tools/setup-update-signing.ps1` made both and refuses to replace an existing key.

Changing this file is a key rotation: every installed copy stops trusting new releases until the
user installs one release by hand. Do it only when the private key is lost or leaked.
