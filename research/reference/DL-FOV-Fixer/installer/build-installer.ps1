<#
.SYNOPSIS
    Publish DL-FOV-Fixer and build the per-user Inno Setup installer, its checksum, manifest.json
    and manifest.sig.

.DESCRIPTION
    Steps: self-contained publish, compile the installer, write the .sha256 and manifest.json beside
    it, then sign manifest.json into manifest.sig with the update channel's key (ADR 0006).

    The manifest key is the PEM in DLFOVFIXER_MANIFEST_SIGNING_KEY, or the file in -ManifestKeyPath
    (on the offline drive). It must match contracts/keys/release-manifest-public.b64, and the
    signature is checked before the script ends. Without -Release a missing key only skips
    manifest.sig. With -Release it is an error, since the app would never trust that release.

    Authenticode signing of the exe and the installer is optional and happens only when
    WINDOWS_CERT_PFX_BASE64 is set (tools/sign-windows-artifacts.ps1). It only quiets SmartScreen:
    the update channel relies on the manifest signature, not on it.

    Without -Release the app is a "-dev" build. With -Release it carries the plain version.

    Output, under <OutputDirectory>: DL-FOV-Fixer-<version>-setup.exe, the same name plus .sha256,
    manifest.json and manifest.sig. The publish folder is <OutputDirectory>/publish. All of it is
    ignored by Git.

.EXAMPLE
    ./installer/build-installer.ps1 -Release -ManifestKeyPath 'E:\DL-FOV-Fixer-signing\release-manifest-signing.pem'
#>
[CmdletBinding()]
param(
    [switch] $Release,
    [string] $OutputDirectory = 'artifacts',
    [string] $IsccPath,
    [string] $ManifestKeyPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$project = Join-Path $repositoryRoot 'src/DlFovFixer.App/DlFovFixer.App.csproj'
$definition = Join-Path $PSScriptRoot 'DL-FOV-Fixer.iss'
$signScript = Join-Path $repositoryRoot 'tools/sign-windows-artifacts.ps1'
$publicKeyFile = Join-Path $repositoryRoot 'contracts/keys/release-manifest-public.b64'
. (Join-Path $repositoryRoot 'tools/openssl.ps1')

$versionLine = Get-Content -LiteralPath (Join-Path $repositoryRoot 'version.properties') |
    Where-Object { $_ -match '^versionName=\d+\.\d+\.\d+\s*$' } | Select-Object -First 1
if (-not $versionLine) { throw 'versionName=X.Y.Z was not found in version.properties.' }
$version = ([regex]::Match($versionLine, '\d+\.\d+\.\d+')).Value

# A relative output directory is relative to the repository root, not to wherever this ran from.
if ([IO.Path]::IsPathRooted($OutputDirectory)) {
    $out = [IO.Path]::GetFullPath($OutputDirectory)
} else {
    $out = [IO.Path]::GetFullPath((Join-Path $repositoryRoot $OutputDirectory))
}
$publishDir = Join-Path $out 'publish'

function Resolve-Iscc {
    param([string] $Explicit)
    if ($Explicit) {
        if (-not (Test-Path -LiteralPath $Explicit -PathType Leaf)) { throw "ISCC.exe not found at '$Explicit'." }
        return (Resolve-Path -LiteralPath $Explicit).Path
    }
    $onPath = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    $candidates = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    )
    foreach ($candidate in $candidates) {
        if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Leaf)) { return $candidate }
    }
    throw 'Inno Setup 6 (ISCC.exe) was not found. Install it (choco install innosetup) or pass -IsccPath.'
}

function Invoke-Signing {
    param([string] $File, [string] $Description)
    if (-not $env:WINDOWS_CERT_PFX_BASE64) {
        Write-Warning "WINDOWS_CERT_PFX_BASE64 is not set, so $(Split-Path -Leaf $File) has no Authenticode signature."
        return
    }
    & $signScript -Path $File -Description $Description -DescriptionUrl 'https://github.com/lukr-99/DL-FOV-Fixer' -Require
}

# The manifest signature is what makes an update trusted (ADR 0006). The key comes from the
# environment in CI or from the offline drive locally, and only ever passes through a temp file. It
# is checked here, before the slow publish, so a missing or wrong key fails at once.
$utf8 = [Text.UTF8Encoding]::new($false)
$expectedPublicKey = (Get-Content -LiteralPath $publicKeyFile -Raw).Trim()
$keyFile = $null
$tempKey = $null
if ($ManifestKeyPath) {
    if (-not (Test-Path -LiteralPath $ManifestKeyPath -PathType Leaf)) { throw "No manifest key at '$ManifestKeyPath'." }
    $keyFile = $ManifestKeyPath
} elseif ($env:DLFOVFIXER_MANIFEST_SIGNING_KEY) {
    $tempKey = [IO.Path]::GetTempFileName()
    [IO.File]::WriteAllText($tempKey, $env:DLFOVFIXER_MANIFEST_SIGNING_KEY.Trim() + "`n", $utf8)
    $keyFile = $tempKey
}

if (-not $keyFile -and $Release) {
    throw 'A release needs the manifest key: set DLFOVFIXER_MANIFEST_SIGNING_KEY or pass -ManifestKeyPath.'
}

if ($keyFile -and (Get-PublicKeyBase64 -PrivateKeyPath $keyFile) -ne $expectedPublicKey) {
    if ($tempKey) { Remove-Item -LiteralPath $tempKey -Force -ErrorAction SilentlyContinue }
    throw "The manifest key does not match $publicKeyFile, so the app would refuse this release."
}

$iscc = Resolve-Iscc -Explicit $IsccPath

if (Test-Path -LiteralPath $publishDir) { Remove-Item -LiteralPath $publishDir -Recurse -Force }
[void][IO.Directory]::CreateDirectory($publishDir)

$publishArgs = @(
    'publish', $project,
    '-c', 'Release',
    '-r', 'win-x64',
    '--self-contained', 'true',
    '-p:PublishSingleFile=false',
    '-p:DebugType=None',
    '-o', $publishDir,
    '--nologo'
)
if ($Release) { $publishArgs += '-p:DlFovFixerReleaseBuild=true' }
Write-Host "Publishing DL-FOV-Fixer $version..." -ForegroundColor Cyan
& dotnet @publishArgs
if ($LASTEXITCODE -ne 0) { throw "dotnet publish failed with exit code $LASTEXITCODE." }

$appExe = Join-Path $publishDir 'DL-FOV-Fixer.exe'
if (-not (Test-Path -LiteralPath $appExe -PathType Leaf)) { throw "The publish did not produce $appExe." }
Invoke-Signing -File $appExe -Description 'DL-FOV-Fixer'

Write-Host 'Compiling the installer...' -ForegroundColor Cyan
& $iscc /Q "/DAppVersion=$version" "/DPublishDir=$publishDir" "/DOutputDir=$out" $definition
if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed with exit code $LASTEXITCODE." }

$setupName = "DL-FOV-Fixer-$version-setup.exe"
$setup = Join-Path $out $setupName
if (-not (Test-Path -LiteralPath $setup -PathType Leaf)) { throw "The installer was not produced: $setup" }
Invoke-Signing -File $setup -Description 'DL-FOV-Fixer setup'

# Hash and size come after signing, because signing changes the file.
$hash = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant()
$size = (Get-Item -LiteralPath $setup).Length
[IO.File]::WriteAllText("$setup.sha256", "$hash  $setupName`n", $utf8)

$manifest = [ordered]@{
    schema      = 1
    version     = $version
    publishedAt = [DateTime]::UtcNow.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'")
    artifacts   = @(
        [ordered]@{
            kind   = 'installer'
            path   = "$version/$setupName"
            size   = $size
            sha256 = $hash
        }
    )
}
# Compact output is the same on Windows PowerShell 5.1 and pwsh 7, unlike the pretty printer.
$json = $manifest | ConvertTo-Json -Depth 5 -Compress
[IO.File]::WriteAllText((Join-Path $out 'manifest.json'), "$json`n", $utf8)

$manifestPath = Join-Path $out 'manifest.json'
$signaturePath = Join-Path $out 'manifest.sig'
if (Test-Path -LiteralPath $signaturePath) { Remove-Item -LiteralPath $signaturePath -Force }

try {
    if (-not $keyFile) {
        Write-Warning 'No manifest key, so there is no manifest.sig. The app will not trust this build as an update.'
    } else {
        $signature = New-ManifestSignature -Path $manifestPath -PrivateKeyPath $keyFile
        [IO.File]::WriteAllText($signaturePath, "$signature`n", $utf8)

        # Check the signature the way the app will, against the public key alone.
        $publicDer = [IO.Path]::GetTempFileName()
        $signatureDer = [IO.Path]::GetTempFileName()
        try {
            [IO.File]::WriteAllBytes($publicDer, [Convert]::FromBase64String($expectedPublicKey))
            [IO.File]::WriteAllBytes($signatureDer, [Convert]::FromBase64String($signature))
            Invoke-OpenSsl @('dgst', '-sha256', '-verify', $publicDer, '-keyform', 'DER', '-signature', $signatureDer, $manifestPath)
        } finally {
            Remove-Item -LiteralPath $publicDer, $signatureDer -Force -ErrorAction SilentlyContinue
        }

        Write-Host 'Signed manifest.json into manifest.sig and checked it.' -ForegroundColor Green
    }
} finally {
    if ($tempKey) { Remove-Item -LiteralPath $tempKey -Force -ErrorAction SilentlyContinue }
}

Write-Host "Built $setup ($([math]::Round($size / 1MB, 1)) MB)." -ForegroundColor Green
