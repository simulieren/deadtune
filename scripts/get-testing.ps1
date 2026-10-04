# Downloads the latest `testing` prerelease into %USERPROFILE%\DeadTune-testing\<commit>.
# Needs the GitHub CLI logged in (repo is private): winget install GitHub.cli; gh auth login
param([switch]$Run)
$ErrorActionPreference = "Stop"
$repo = "simulieren/deadtune"
$sha = (gh release view testing -R $repo --json targetCommitish -q .targetCommitish).Substring(0, 7)
$dir = Join-Path $env:USERPROFILE "DeadTune-testing\$sha"
if (-not (Test-Path $dir)) {
    $tmp = Join-Path $env:TEMP "deadtune-$sha"
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
    gh release download testing -R $repo -D $tmp -p "deadtune-windows-x64.zip*"
    $want = (Get-Content "$tmp\deadtune-windows-x64.zip.sha256").Split(" ")[0]
    $got = (Get-FileHash "$tmp\deadtune-windows-x64.zip" -Algorithm SHA256).Hash.ToLower()
    if ($want -ne $got) { throw "checksum mismatch: $got != $want" }
    Expand-Archive "$tmp\deadtune-windows-x64.zip" $dir
    Get-ChildItem $dir | Unblock-File
}
Write-Host "DeadTune testing build $sha in $dir"
if ($Run) { Start-Process (Join-Path $dir "deadtune.exe") } else { explorer $dir }
