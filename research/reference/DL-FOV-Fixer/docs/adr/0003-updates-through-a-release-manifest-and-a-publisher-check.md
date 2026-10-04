# ADR 0003: Updates through a release manifest and an Authenticode publisher check

The 1.x updater read the GitHub API, accepted any `.exe` asset it found and copied it over its own
running executable, which trusts the release listing for both what to download and whether it is safe
to run. From 2.0.0 the channel is a `manifest.json` asset on the latest release that names the
version and, for each artifact, its file name, size and SHA-256; the downloaded installer must match
that size and hash, and its Authenticode signature must be valid and carry the pinned publisher
before the installer is allowed to start. GoalMaker goes one step further and signs the manifest
itself with ECDSA P-256, which this app does not need yet: tampering with the manifest requires
publishing to the repository's releases, and the publisher check still gates execution afterwards, so
the added key management would buy little here. That extra signature stays purely additive, and the
stages (channel, version policy, artifact selection, verified download, installer launch) are each
kept behind their own seam so it can be inserted without touching the rest.

The Authenticode publisher check is replaced by ADR 0006: the manifest signature this ADR called
an additive step is now what the update channel relies on, because no certificate is bought.
