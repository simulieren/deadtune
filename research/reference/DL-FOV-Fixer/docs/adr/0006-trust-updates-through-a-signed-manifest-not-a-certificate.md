# ADR 0006: Trust updates through a signed manifest, not a code-signing certificate

ADR 0003 made the 2.x updater run an installer only when its Authenticode signature carried a pinned
publisher, which needs a paid code-signing certificate, and the owner decided on 2026-09-30 not to
buy one. From now on the release workflow signs the exact bytes of `manifest.json` with an ECDSA
P-256 key (DER signature, base64, in `manifest.sig`), the app builds in the public key from
`contracts/keys/release-manifest-public.b64`, and it refuses a release whose signature does not
verify before it reads the manifest at all; the downloaded installer is then trusted because its
size and SHA-256 match that signed manifest, which is the same proof a publisher check gave, at no
cost, and it is the scheme GoalMaker already ships. The private key was generated straight onto the
owner's offline flash drive (`E:\DL-FOV-Fixer-signing`, `tools/setup-update-signing.ps1`) and lives
otherwise only in the `DLFOVFIXER_MANIFEST_SIGNING_KEY` Actions secret, so whoever controls that
secret or the GitHub account can ship an update, the same trust a certificate kept in CI would have
had; losing the key means a new one and one manual update for every user. What this does not buy is
a quiet first download: without a certificate SmartScreen still warns once per version when a user
downloads the installer in a browser, while an update the app downloads itself carries no
mark-of-the-web and is not stopped by SmartScreen. Authenticode signing stays possible through
`WINDOWS_CERT_PFX_BASE64` for that first-download reason alone, and the update channel no longer
depends on it.
