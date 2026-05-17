# Avatar Studio — generate the self-signed dev code-signing cert.
#
# Run once on a fresh dev machine. Writes a PFX to
#   %USERPROFILE%\.avatar_studio\dev-cert.pfx
# with the password from $env:AVATAR_STUDIO_PFX_PASS (or "avatar-studio-dev"
# if the env var is unset).
#
# The resulting cert is "Avatar Studio Dev" — only trusted on machines that
# explicitly install it into the Trusted Root Certification Authorities
# store. Production builds should swap in an OV/EV cert via `build_installer.ps1`.

[CmdletBinding()]
param(
    [string]$Subject = "CN=Avatar Studio Dev",
    [string]$Out     = "$env:USERPROFILE\.avatar_studio\dev-cert.pfx",
    [string]$Password = $env:AVATAR_STUDIO_PFX_PASS,
    [int]   $Years    = 3
)

$ErrorActionPreference = "Stop"
if (-not $Password) {
    $Password = "avatar-studio-dev"
    Write-Warning "AVATAR_STUDIO_PFX_PASS not set; using default password 'avatar-studio-dev'."
    Write-Warning "Set the env var before running build_installer.ps1 in CI."
}

$dir = Split-Path $Out -Parent
if (-not (Test-Path $dir)) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
}

Write-Host "Creating self-signed code-signing cert: $Subject"
$cert = New-SelfSignedCertificate `
    -Type CodeSigning `
    -Subject $Subject `
    -KeyUsage DigitalSignature `
    -KeyAlgorithm RSA -KeyLength 2048 `
    -NotAfter (Get-Date).AddYears($Years) `
    -CertStoreLocation Cert:\CurrentUser\My

$pwd = ConvertTo-SecureString -String $Password -Force -AsPlainText
Export-PfxCertificate -Cert $cert -FilePath $Out -Password $pwd | Out-Null
Write-Host "Exported PFX → $Out"
Write-Host ("Thumbprint  : {0}" -f $cert.Thumbprint)
Write-Host ("NotAfter    : {0}" -f $cert.NotAfter)
Write-Host ""
Write-Host "To silence SmartScreen on this machine, install the PFX into"
Write-Host "  Cert:\CurrentUser\Root  (or the LocalMachine Root store if you're admin)."
Write-Host "See docs/packaging.md for the one-liner."
