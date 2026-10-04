<#
.SYNOPSIS
    Enroll an Authenticode code-signing certificate into this repository's GitHub Actions secrets.

.DESCRIPTION
    Run this once, yourself, after obtaining a certificate. It validates the certificate, then
    uploads it so release workflows can sign unattended. The password is typed hidden and is never
    printed, logged, or placed on a command line, so this is safe to run with an agent watching.

    After this, signing needs no further human involvement: an agent cuts releases by pushing a
    tag, and the workflow signs from these secrets.

    Uploads: WINDOWS_CERT_PFX_BASE64, WINDOWS_CERT_PASSWORD.

.EXAMPLE
    ./tools/setup-windows-signing.ps1 -PfxPath 'E:\signing\codesign.pfx'

.EXAMPLE
    # Hardware token or cloud HSM: the key cannot be exported, so record only the thumbprint.
    ./tools/setup-windows-signing.ps1 -Thumbprint 'A1B2C3...'

.NOTES
    Requires the GitHub CLI `gh`, authenticated. See docs/release-signing.md for how to obtain a
    certificate - since June 2023 a publicly-trusted one cannot be issued as a plain PFX.
#>
[CmdletBinding(DefaultParameterSetName = "Pfx")]
param(
    [Parameter(Mandatory, ParameterSetName = "Pfx")][string] $PfxPath,
    [Parameter(Mandatory, ParameterSetName = "Thumbprint")][string] $Thumbprint,
    # Check the certificate but upload nothing.
    [switch] $WhatIfOnly
)

$ErrorActionPreference = "Stop"
$Utf8NoBom = New-Object System.Text.UTF8Encoding $false

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { throw "gh CLI not found on PATH." }

if ($PSCmdlet.ParameterSetName -eq "Thumbprint") {
    $certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$Thumbprint" -ErrorAction SilentlyContinue
    if (-not $certificate) { throw "No certificate with thumbprint $Thumbprint in Cert:\CurrentUser\My." }
    Write-Host "Found: $($certificate.Subject)"
    Write-Host "Expires: $($certificate.NotAfter.ToString('yyyy-MM-dd'))"
    Write-Host ""
    Write-Host "A hardware-backed key cannot be uploaded as a secret. Set WINDOWS_CERT_THUMBPRINT on" -ForegroundColor Yellow
    Write-Host "a self-hosted runner that has the token attached, or switch the workflow to your" -ForegroundColor Yellow
    Write-Host "provider's signing action (Azure Trusted Signing, SSL.com eSigner, DigiCert KeyLocker)." -ForegroundColor Yellow
    Write-Host "Thumbprint: $Thumbprint"
    return
}

if (-not (Test-Path -LiteralPath $PfxPath)) { throw "No PFX at $PfxPath." }

$password = Read-Host "PFX password (hidden)" -AsSecureString

# Load it before uploading: an unreadable or already-expired certificate that only fails inside CI
# costs a whole release cycle to discover.
$bytes = [IO.File]::ReadAllBytes($PfxPath)
try {
    $certificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new(
        $bytes, $password, [System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::EphemeralKeySet)
} catch {
    throw "Could not open the PFX. Wrong password, or the file is not a PFX: $($_.Exception.Message)"
}

Write-Host "Subject:  $($certificate.Subject)"
Write-Host "Issuer:   $($certificate.Issuer)"
Write-Host "Expires:  $($certificate.NotAfter.ToString('yyyy-MM-dd'))"

if (-not $certificate.HasPrivateKey) { throw "This PFX has no private key, so it cannot sign." }
if ($certificate.NotAfter -lt (Get-Date)) { throw "This certificate expired on $($certificate.NotAfter.ToString('yyyy-MM-dd'))." }
if ($certificate.NotAfter -lt (Get-Date).AddDays(30)) {
    Write-Warning "This certificate expires in under 30 days. Timestamped signatures stay valid, but you cannot sign new artifacts after it lapses."
}

# Code Signing EKU is 1.3.6.1.5.5.7.3.3. A TLS certificate loads fine here but produces signatures
# Windows rejects, so catch the mix-up now rather than in a release.
$enhancedKeyUsage = $certificate.Extensions | Where-Object { $_ -is [System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension] }
if ($enhancedKeyUsage) {
    $oids = @($enhancedKeyUsage.EnhancedKeyUsages | ForEach-Object { $_.Value })
    if ($oids -notcontains "1.3.6.1.5.5.7.3.3") {
        throw "This certificate is not valid for code signing (no Code Signing EKU). Found: $($oids -join ', ')"
    }
}

if ($certificate.Issuer -eq $certificate.Subject) {
    Write-Warning "This certificate is self-signed. It will NOT stop SmartScreen warnings - the chain is not publicly trusted."
}

if ($WhatIfOnly) { Write-Host "-WhatIfOnly: nothing uploaded." -ForegroundColor Cyan; return }

# Values go through a short-lived dotenv file rather than a pipe: Windows PowerShell appends a
# newline to anything piped into a native command, which would become part of the password, and
# --body would put it on gh's command line.
$plainPassword = [Runtime.InteropServices.Marshal]::PtrToStringBSTR(
    [Runtime.InteropServices.Marshal]::SecureStringToBSTR($password))
$secretsFile = [IO.Path]::Combine([IO.Path]::GetTempPath(), "codesign-secrets-$([guid]::NewGuid().ToString('N')).env")
try {
    $lines = "WINDOWS_CERT_PFX_BASE64=$([Convert]::ToBase64String($bytes))`nWINDOWS_CERT_PASSWORD=$plainPassword`n"
    [IO.File]::WriteAllText($secretsFile, $lines, $Utf8NoBom)
    & gh secret set --env-file $secretsFile
    if ($LASTEXITCODE -ne 0) { throw "gh secret set failed with exit code $LASTEXITCODE" }
}
finally {
    Remove-Item -LiteralPath $secretsFile -Force -ErrorAction SilentlyContinue
    $plainPassword = $null
}

Write-Host "Uploaded WINDOWS_CERT_PFX_BASE64 and WINDOWS_CERT_PASSWORD." -ForegroundColor Green
Write-Host "Keep the PFX and its password in offline storage and a password manager - GitHub secrets cannot be read back." -ForegroundColor Red
Write-Host "Next: add -Require to the signing step so releases fail rather than publish unsigned." -ForegroundColor Cyan
