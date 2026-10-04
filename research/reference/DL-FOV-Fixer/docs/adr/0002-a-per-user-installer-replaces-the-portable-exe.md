# ADR 0002: A signed per-user installer replaces the portable exe

Version 1.x shipped one portable executable that could be dropped anywhere, which is friendly but
gives Windows nothing to trust and gives an updater nowhere safe to write. From 2.0.0 the release
artifact is a per-user Inno Setup installer built from a deterministic self-contained publish, with
a fixed AppId, a startup task that writes the same Run value the app's own setting uses,
`CloseApplications` limited to the app's executable so the Restart Manager can close the tray app
during an in-place update, and a SHA-256 written beside the installer. Self-contained keeps the
promise that the app works the moment it is installed, with no .NET runtime prerequisite; inside the
installer the publish stays a plain folder rather than a single-file bundle, because extracting a
bundle into a temporary folder at every start is the exact behavior that earned the Python build its
trojan verdict. Uninstall leaves `%APPDATA%\DL-FOV-Fixer` in place so a reinstall keeps the chosen
FOV and the stored tweaks, and the trade-off accepted here is the lost drop-anywhere property, which
only three recorded downloads relied on and which can return later as a separately named portable
asset.
