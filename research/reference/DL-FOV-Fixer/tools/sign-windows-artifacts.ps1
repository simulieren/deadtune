<#
.SYNOPSIS
    Authenticode-sign Windows release artifacts. Skips cleanly when no certificate is configured.

.DESCRIPTION
    The signing identity never reaches the caller. It is read from the environment, used, and
    discarded here, so a release workflow (or an agent driving one) can produce signed artifacts
    without ever holding certificate material.

    Certificate sources, in order:
      1. WINDOWS_CERT_PFX_BASE64 (+ WINDOWS_CERT_PASSWORD) - a PFX carried in a CI secret.
      2. WINDOWS_CERT_THUMBPRINT - a certificate already in the current user's store, which is how
         a hardware token or a cloud-HSM provider (Azure Trusted Signing, SSL.com eSigner,
         DigiCert KeyLocker) exposes a key that cannot be exported into a secret.

    Fail-closed, not fail-open: if a certificate IS configured and signing fails, this throws. The
    only quiet path is "no certificate configured at all", which warns and returns so the release
    still builds while a certificate is being obtained. Pass -Require once you have one, to turn
    that last quiet path into an error.

.EXAMPLE
    ./tools/sign-windows-artifacts.ps1 -Path dist/App.exe -Description 'App'

.EXAMPLE
    # Ready for production: refuse to continue unless the artifacts really are signed.
    ./tools/sign-windows-artifacts.ps1 -Path 'publish/*.exe','publish/*.dll' -Require

.NOTES
    Requires signtool.exe (Windows SDK). Present on GitHub's windows-latest runners.
#>
[CmdletBinding()]
param(
    # Files to sign. Wildcards allowed. Entries matching nothing are skipped unless -Require.
    [Parameter(Mandatory)][string[]] $Path,
    # Shown in the UAC prompt. Defaults to the file name.
    [string] $Description,
    [string] $DescriptionUrl,
    # RFC 3161 timestamp server. Timestamping is what keeps a signature valid after the
    # certificate expires; without it every artifact silently goes bad on the expiry date.
    [string] $TimestampUrl = "http://timestamp.digicert.com",
    # Treat "no certificate configured" and "no files matched" as errors rather than warnings.
    [switch] $Require
)

$ErrorActionPreference = "Stop"

function Resolve-SignTool {
    $onPath = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }

    # Windows SDKs install side by side; take the newest x64 build rather than guessing a version.
    $roots = @("${env:ProgramFiles(x86)}\Windows Kits\10\bin", "$env:ProgramFiles\Windows Kits\10\bin")
    $candidates = foreach ($root in $roots) {
        if (Test-Path $root) {
            Get-ChildItem -Path $root -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
                Where-Object { $_.FullName -like '*\x64\*' }
        }
    }
    # The SDK version is the grandparent directory (…\bin\10.0.26100.0\x64\signtool.exe). Sort by it
    # as a version, and fall back to 0.0.0.0 for any layout that does not carry one.
    $newest = $candidates | Sort-Object {
        $parsed = [version]::new()
        if ([version]::TryParse($_.Directory.Parent.Name, [ref]$parsed)) { $parsed } else { [version]"0.0.0.0" }
    } -Descending | Select-Object -First 1
    if (-not $newest) { throw "signtool.exe not found. Install the Windows SDK signing tools." }
    return $newest.FullName
}

$files = @()
foreach ($pattern in $Path) {
    $matched = @(Get-ChildItem -Path $pattern -File -ErrorAction SilentlyContinue)
    if ($matched.Count -eq 0) {
        $message = "No files matched '$pattern'."
        if ($Require) { throw $message }
        Write-Warning $message
        continue
    }
    $files += $matched
}
if ($files.Count -eq 0) {
    if ($Require) { throw "Nothing to sign." }
    Write-Warning "Nothing to sign."
    return
}

$pfxBase64 = $env:WINDOWS_CERT_PFX_BASE64
$thumbprint = $env:WINDOWS_CERT_THUMBPRINT

if (-not $pfxBase64 -and -not $thumbprint) {
    $message = @(
        "No signing certificate configured - artifacts will be published UNSIGNED.",
        "Windows SmartScreen and Defender will warn about them, and PyInstaller-built",
        "executables are frequently reported as trojans outright.",
        "Set WINDOWS_CERT_PFX_BASE64 + WINDOWS_CERT_PASSWORD, or WINDOWS_CERT_THUMBPRINT.",
        "See docs/release-signing.md for how to obtain a certificate."
    ) -join " "
    if ($Require) { throw $message }
    Write-Warning $message
    return
}

$signTool = Resolve-SignTool
$importedThumbprint = $null
$pfxPath = $null

try {
    if ($pfxBase64) {
        # Import the PFX into the store and sign by thumbprint instead of passing signtool /f /p.
        # signtool's /p puts the password on a command line that any other process on the machine
        # can read out of the process list; the store import keeps it in this process only.
        $pfxPath = [IO.Path]::Combine([IO.Path]::GetTempPath(), "codesign-$([guid]::NewGuid().ToString('N')).pfx")
        [IO.File]::WriteAllBytes($pfxPath, [Convert]::FromBase64String($pfxBase64))

        $securePassword = if ($env:WINDOWS_CERT_PASSWORD) {
            ConvertTo-SecureString $env:WINDOWS_CERT_PASSWORD -AsPlainText -Force
        } else {
            New-Object System.Security.SecureString
        }
        $imported = Import-PfxCertificate -FilePath $pfxPath -CertStoreLocation Cert:\CurrentUser\My -Password $securePassword
        $importedThumbprint = $imported.Thumbprint
        $thumbprint = $imported.Thumbprint
        Write-Host "Imported signing certificate $($imported.Subject) (expires $($imported.NotAfter.ToString('yyyy-MM-dd')))."

        if ($imported.NotAfter -lt (Get-Date)) { throw "The signing certificate expired on $($imported.NotAfter.ToString('yyyy-MM-dd'))." }
    }

    foreach ($file in $files) {
        $arguments = @(
            "sign",
            "/sha1", $thumbprint,
            "/fd", "sha256",
            "/tr", $TimestampUrl,
            "/td", "sha256",
            "/d", $(if ($Description) { $Description } else { $file.BaseName })
        )
        if ($DescriptionUrl) { $arguments += @("/du", $DescriptionUrl) }
        $arguments += $file.FullName

        & $signTool @arguments
        if ($LASTEXITCODE -ne 0) { throw "signtool failed on $($file.Name) with exit code $LASTEXITCODE." }
    }

    # Verify against the real trust chain rather than trusting signtool's own exit code. /pa uses
    # the Authenticode policy, which is what Windows itself applies when a user runs the file.
    foreach ($file in $files) {
        & $signTool verify /pa /q $file.FullName
        if ($LASTEXITCODE -ne 0) {
            throw "$($file.Name) did not verify after signing. Do not publish it."
        }
        Write-Host "Signed and verified: $($file.Name)" -ForegroundColor Green
    }
}
finally {
    if ($importedThumbprint) {
        Remove-Item -LiteralPath "Cert:\CurrentUser\My\$importedThumbprint" -Force -ErrorAction SilentlyContinue
    }
    if ($pfxPath) { Remove-Item -LiteralPath $pfxPath -Force -ErrorAction SilentlyContinue }
}
