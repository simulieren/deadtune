<#
.SYNOPSIS
    Creates the update channel's manifest signing key (ADR 0006), once.

.DESCRIPTION
      1. Generates an ECDSA P-256 private key with OpenSSL straight into -KeyFolder, by default the
         offline flash drive. It refuses to replace an existing key.
      2. Writes the public key to contracts/keys/release-manifest-public.b64. Commit that file: the
         app builds it in and trusts only manifests signed by this key.
      3. Writes a README.txt beside the key with the public key and the key file's SHA-256.
      4. With -UploadSecret, uploads the private key as the DLFOVFIXER_MANIFEST_SIGNING_KEY GitHub
         Actions secret, which the release workflow signs with.

    The private key is not password-protected, because CI needs it unattended, so keep the folder
    offline. Losing it means a new key and one manual update for every user. Leaking it means
    rotating it the same way.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File tools\setup-update-signing.ps1 -KeyFolder 'E:\DL-FOV-Fixer-signing'
#>
[CmdletBinding()]
param(
    [string] $KeyFolder = 'E:\DL-FOV-Fixer-signing',
    [switch] $UploadSecret
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$publicKeyFile = Join-Path $repoRoot 'contracts\keys\release-manifest-public.b64'
$privateKey = Join-Path $KeyFolder 'release-manifest-signing.pem'
$utf8NoBom = New-Object System.Text.UTF8Encoding $false

. (Join-Path $PSScriptRoot 'openssl.ps1')

if (Test-Path -LiteralPath $privateKey) {
    Write-Host "A signing key already exists at $privateKey, keeping it." -ForegroundColor Yellow
} else {
    New-Item -ItemType Directory -Force -Path $KeyFolder | Out-Null
    Invoke-OpenSsl @('ecparam', '-name', 'prime256v1', '-genkey', '-noout', '-out', $privateKey)
    Write-Host "Created $privateKey" -ForegroundColor Green
}

$publicBase64 = Get-PublicKeyBase64 -PrivateKeyPath $privateKey

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $publicKeyFile) | Out-Null
if ((Test-Path -LiteralPath $publicKeyFile) -and ((Get-Content -LiteralPath $publicKeyFile -Raw).Trim() -ne $publicBase64)) {
    throw "$publicKeyFile holds a DIFFERENT public key. Rotating keys is deliberate: delete that file first."
}
[IO.File]::WriteAllText($publicKeyFile, "$publicBase64`n", $utf8NoBom)
Write-Host "Wrote $publicKeyFile, commit it." -ForegroundColor Green

$readme = @(
    'DL-FOV-Fixer update channel: release manifest signing key (ECDSA P-256, PEM)',
    "Public key (also in contracts/keys/release-manifest-public.b64): $publicBase64",
    "File SHA-256: $((Get-FileHash -Algorithm SHA256 -LiteralPath $privateKey).Hash)",
    '',
    'CI signs every 2.x release manifest with this key (GitHub secret DLFOVFIXER_MANIFEST_SIGNING_KEY).',
    'The app installs an update only when its manifest is signed by it. Keep this folder offline.',
    'Losing the key means a new one and one manual update for every user (docs/adr/0006).'
)
[IO.File]::WriteAllText((Join-Path $KeyFolder 'README.txt'), (($readme -join "`r`n") + "`r`n"), $utf8NoBom)

if ($UploadSecret) {
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { throw 'gh CLI not found on PATH.' }
    Get-Content -LiteralPath $privateKey -Raw | & gh secret set DLFOVFIXER_MANIFEST_SIGNING_KEY --repo lukr-99/DL-FOV-Fixer
    if ($LASTEXITCODE -ne 0) { throw "gh secret set failed with exit code $LASTEXITCODE" }
    Write-Host 'Uploaded DLFOVFIXER_MANIFEST_SIGNING_KEY.' -ForegroundColor Green
}
