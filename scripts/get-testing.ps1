# Downloads a DeadTune build into %USERPROFILE%\DeadTune-testing\<tag or commit>.
#   get-testing.ps1              latest rolling `testing` build
#   get-testing.ps1 -Tag latest  newest versioned release (vX.Y.0)
#   get-testing.ps1 -Tag v0.2.0  a specific release
# Needs the GitHub CLI logged in (repo is private): winget install GitHub.cli; gh auth login
param([switch]$Run, [string]$Tag = "testing")
$ErrorActionPreference = "Stop"
$repo = "simulieren/deadtune"
if ($Tag -eq "latest") { $Tag = (gh release view -R $repo --json tagName -q .tagName) }
$asset = (gh release view $Tag -R $repo --json assets -q '.assets[].name' | Where-Object { $_ -like "*.zip" } | Select-Object -First 1)
$name = if ($Tag -eq "testing") { (gh release view testing -R $repo --json targetCommitish -q .targetCommitish).Substring(0, 7) } else { $Tag }
$dir = Join-Path $env:USERPROFILE "DeadTune-testing\$name"
if (-not (Test-Path $dir)) {
    $tmp = Join-Path $env:TEMP "deadtune-$name"
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
    gh release download $Tag -R $repo -D $tmp -p "$asset*"
    $want = (Get-Content "$tmp\$asset.sha256").Split(" ")[0]
    $got = (Get-FileHash "$tmp\$asset" -Algorithm SHA256).Hash.ToLower()
    if ($want -ne $got) { throw "checksum mismatch: $got != $want" }
    Expand-Archive "$tmp\$asset" $dir
    Get-ChildItem $dir -Recurse | Unblock-File
}
Write-Host "DeadTune $name in $dir"
if ($Run) { Start-Process (Join-Path $dir "deadtune.exe") } else { explorer $dir }
