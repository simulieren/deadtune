# Contributing

## Getting the dotnetlib packages

The tray shell uses `DotNetLib.Tray` from the private `dotnetlib` repository. It comes from
dotnetlib's GitHub Packages feed, the `dotnetlib` source in `nuget.config`. Restore needs a token
with the `read:packages` scope, so nobody can build the app without one. Store it once in your user
NuGet config, never in this repository:

```powershell
gh auth refresh -s read:packages
dotnet nuget add source https://nuget.pkg.github.com/lukr-99/index.json --name dotnetlib --username lukr-99 --password (gh auth token)
```

CI and the release workflow read the `DOTNETLIB_PACKAGES_TOKEN` repository secret, a classic PAT
with `read:packages` only, through NuGet's `NuGetPackageSourceCredentials_dotnetlib` variable.

## Local verification

Run what your change touches. CI (`.github/workflows/ci.yml`) runs the same checks on every pull
request, and the release workflow runs the tests again on a tag.

```powershell
dotnet format DL-FOV-Fixer.slnx --verify-no-changes
dotnet build DL-FOV-Fixer.slnx -c Release
dotnet test --solution DL-FOV-Fixer.slnx -c Release --no-build
py ..\CodePrint\tools\validate_repository.py --root .
```

The .NET SDK is pinned in `global.json`. The version lives in `version.properties`, and a build is
`-dev` unless it is made with `-p:DlFovFixerReleaseBuild=true`.

Use `py`, never `python`, for the validator. The `python` command can resolve to the Microsoft Store
build, which writes to a private copy of `%APPDATA%` (see [docs/pitfalls.md](docs/pitfalls.md)).

To run the app without touching your real settings or game files, point it at a scratch config:

```powershell
dotnet run --project src/DlFovFixer.App -- --settings C:\Temp\dlfov-test\config.json
```

## Building the installer

The app ships as a per-user Inno Setup installer. You need the .NET SDK from `global.json` and
Inno Setup 6 (`choco install innosetup`).

```powershell
.\installer\build-installer.ps1            # dev build, version ends in -dev
.\installer\build-installer.ps1 -Release -ManifestKeyPath 'E:\DL-FOV-Fixer-signing\release-manifest-signing.pem'
```

The script publishes the app, compiles `installer/DL-FOV-Fixer.iss`, and writes these files to
`artifacts/` (ignored by Git): `DL-FOV-Fixer-<version>-setup.exe`, its `.sha256`, `manifest.json`
and `manifest.sig`. Pass `-IsccPath` if `ISCC.exe` is not on `PATH` or in the usual install folders.

`manifest.sig` is what makes a release installable through the app's updater (ADR 0006). The script
signs with the key in `-ManifestKeyPath` or the `DLFOVFIXER_MANIFEST_SIGNING_KEY` environment
variable, checks that it matches `contracts/keys/release-manifest-public.b64` before it builds
anything, and checks the signature before it ends. A dev build without a key only skips the
signature, and `-Release` without one fails. `tools/setup-update-signing.ps1` made the key once, see
[contracts/keys/README.md](contracts/keys/README.md).

Authenticode signing is optional and only happens when `WINDOWS_CERT_PFX_BASE64` (and
`WINDOWS_CERT_PASSWORD`) are set. It only quiets SmartScreen on a browser download. Do not run the
installer on your own machine to test a build unless you want it installed. Never change the `AppId`
in the `.iss` file.

## Releasing

1. Bump `version.properties` and add the version to [CHANGELOG.md](CHANGELOG.md) in the same commit.
2. Merge to `main` with CI green.
3. Push the tag, for example `git tag -a v2.0.1 -m "..."` then `git push origin v2.0.1`.
   `.github/workflows/release-windows.yml` tests, builds, signs the manifest and creates a draft
   release. A tag that does not match `version.properties` fails.
4. Check the draft: the installer is the only `.exe`, and `manifest.sig` verifies against
   `contracts/keys/release-manifest-public.b64`.
5. Publish it and mark it Latest. Installed copies find it through `releases/latest`.

## Changing the merge

`gameinfo.gi` is a file the game must still be able to parse, and the user's install is their only
copy. A change to the merge keeps every rule in [AGENTS.md](AGENTS.md), and gets a case that proves
it: a vector in `contracts/vectors/`. See [contracts/vectors/README.md](contracts/vectors/README.md)
for the format and how to add a case.

Never reformat the file, never rewrite a key that already exists as a sub-block, and never overwrite
an existing `.dlfovfixer.bak`.

## Change shape

- One coherent behavior or repository change per commit. Split independent slices.
- Conventional Commits: `type(optional-scope): imperative summary`.
- Tests and the documentation that explains a behavior belong in the same commit as that behavior.
- No commit leaves the repository failing.
- Plain English, and no em-dashes.

## Tracking

Work is tracked on this repository's GoalMaker project. Move an item to Doing when you start it, list
the items in a pull request as `GoalMaker: <item id>` lines, and let the merge to `main` close them.
