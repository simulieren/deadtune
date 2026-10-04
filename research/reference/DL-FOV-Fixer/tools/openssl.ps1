# Helpers for the OpenSSL that ships with Git for Windows, shared by setup-update-signing.ps1 and
# installer/build-installer.ps1. Dot-source this file.

function Get-OpenSslPath {
    $onPath = Get-Command openssl -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    $gitOpenssl = Join-Path $env:ProgramFiles 'Git\usr\bin\openssl.exe'
    if (Test-Path -LiteralPath $gitOpenssl) { return $gitOpenssl }
    throw 'openssl not found. It ships with Git for Windows.'
}

# OpenSSL reports progress on stderr, which Windows PowerShell 5.1 turns into a terminating error
# under 'Stop', so it runs with 'Continue' and is judged by its exit code instead.
function Invoke-OpenSsl([string[]] $Arguments) {
    $openssl = Get-OpenSslPath
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $output = & $openssl @Arguments 2>&1
        if ($LASTEXITCODE -ne 0) { throw "openssl $($Arguments[0]) failed with exit code $LASTEXITCODE`: $output" }
    } finally {
        $ErrorActionPreference = $previous
    }
}

# The public key as base64 DER SubjectPublicKeyInfo, the form the app builds in.
function Get-PublicKeyBase64([string] $PrivateKeyPath) {
    $temp = [IO.Path]::GetTempFileName()
    try {
        Invoke-OpenSsl @('ec', '-in', $PrivateKeyPath, '-pubout', '-outform', 'DER', '-out', $temp)
        return [Convert]::ToBase64String([IO.File]::ReadAllBytes($temp))
    } finally {
        Remove-Item -LiteralPath $temp -Force -ErrorAction SilentlyContinue
    }
}

# Signs the exact bytes of $Path with ECDSA P-256 and SHA-256, and returns the DER signature as
# base64: the manifest.sig format the app checks (ADR 0006).
function New-ManifestSignature([string] $Path, [string] $PrivateKeyPath) {
    $temp = [IO.Path]::GetTempFileName()
    try {
        Invoke-OpenSsl @('dgst', '-sha256', '-sign', $PrivateKeyPath, '-out', $temp, $Path)
        return [Convert]::ToBase64String([IO.File]::ReadAllBytes($temp))
    } finally {
        Remove-Item -LiteralPath $temp -Force -ErrorAction SilentlyContinue
    }
}
